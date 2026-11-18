//! The `ci decide` subcommand.
//!
//! Deciding is a pipeline of small steps: collect the CLI/environment context,
//! derive unconditional overrides, evaluate the deterministic graph for every
//! job, make at most one provider request covering them all, and record the
//! results. Keeping each step separate makes the reason string auditable,
//! which is the part CI operators actually read.
//!
//! The pipeline is wrapped by an error policy: `--on-error fail` (the default)
//! keeps failures loud, while `--on-error run-all` fails open because GitHub
//! skips dependent jobs when the gate fails, and a skipped required check
//! counts as passing.

mod input;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use chrono::Utc;
use clap::ArgMatches;
use input::DecideInput;
use input::parse_decide_input;
use monochange_config::load_ci_settings;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use super::UNCONDITIONAL_MARKERS;
use super::args;
use super::auth;
use super::graph::GraphContext;
use super::graph::GraphVerdict;
use super::graph::VerdictSource;
use super::graph::duration_threshold_minutes;
use super::graph::evaluate_graph_context;
use super::graph::evaluate_graph_decision_for_job;
use super::graph::exceeds_duration_threshold;
use super::graph::is_main_branch;
use super::graph::is_rust_job_or_context;
use super::graph::matches_any_glob;
use super::graph::optional_duration_suggestions;
use super::provider::DecisionPrompt;
use super::provider::PromptJob;
use super::provider::ProviderDecision;
use super::provider::ProviderOverrides;
use super::provider::default_model_for;
use super::provider::query_external_decision_provider;
use super::render::render_decide_report;
use super::render::render_fail_safe_outputs;
use super::render::write_github_outputs;
use super::types::CiDecideReport;
use super::types::CiDecisionProvider;
use super::types::CiDecisionRecord;
use super::types::CiMemoryStore;

/// Provider label recorded for the deterministic workspace graph.
const GRAPH_PROVIDER_LABEL: &str = "graph";
/// Model label recorded for the deterministic workspace graph.
const GRAPH_MODEL_LABEL: &str = "workspace-graph";
/// Suffix appended when an external provider fails and the graph takes over.
const GRAPH_FALLBACK_SUFFIX: &str = " (evaluated via deterministic graph fallback)";
/// How many recent decisions are quoted in a provider prompt.
const RECENT_HISTORY_LIMIT: usize = 5;
/// GitHub annotation prefix that renders the error on the workflow run.
const FAIL_SAFE_ANNOTATION: &str = "::error title=monochange ci decide::";

/// Reaction to pipeline errors after argument parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnError {
	/// Exit non-zero so the failure is loud.
	Fail,
	/// Emit run-all outputs and exit 0 so every gated job still runs.
	RunAll,
}

/// One job's forced overrides and deterministic graph verdict.
struct PreparedJob {
	id: String,
	/// Rule 1 and rule 3 reasons; a non-empty list forces a run.
	unconditional_reasons: Vec<String>,
	matched_learned_rules: Vec<String>,
	/// Rule 2: `[ci.jobs.<id>].always`.
	always: bool,
	graph: GraphVerdict,
}

/// Provider verdict for a single job.
struct JobOutcome {
	run: bool,
	reason: String,
	provider: String,
	model: String,
}

/// Result of the single provider phase for a whole decide run.
enum ProviderPhase {
	/// No provider request was needed or configured.
	LocalGraph,
	/// The provider responded with per-job decisions.
	Responded(BTreeMap<String, ProviderDecision>, String, String),
	/// The provider call failed; the graph verdict stands.
	Failed,
}

/// Duration history and red-flag details for a single job.
struct DurationAnalysis {
	red_flag: bool,
	observed_seconds: Option<u64>,
	threshold_minutes: u64,
	suggestions: Vec<String>,
}

/// Evaluate every requested job and persist the resulting decisions.
pub(super) async fn run_ci_decide_subcommand(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	let github_output = github_output_path();
	let outcome = decide_pipeline(root, matches, github_output.as_deref()).await;

	match outcome {
		Ok(output) => Ok(output),
		Err(error) => {
			match on_error_arg(matches) {
				OnError::Fail => Err(error),
				OnError::RunAll => {
					fail_safe_decide(root, matches, github_output.as_deref(), &error)
				}
			}
		}
	}
}

/// Read `$GITHUB_OUTPUT` once per decide run; empty values count as unset.
fn github_output_path() -> Option<PathBuf> {
	std::env::var_os("GITHUB_OUTPUT")
		.filter(|value| !value.is_empty())
		.map(PathBuf::from)
}

/// Parse the `--on-error` flag; clap restricts the accepted values.
fn on_error_arg(matches: &ArgMatches) -> OnError {
	if matches
		.get_one::<String>("on-error")
		.is_some_and(|value| value == "run-all")
	{
		return OnError::RunAll;
	}

	OnError::Fail
}

/// Run the full decision pipeline, propagating every step's errors.
async fn decide_pipeline(
	root: &Path,
	matches: &ArgMatches,
	github_output: Option<&Path>,
) -> MonochangeResult<String> {
	let input = parse_decide_input(root, matches, github_output)?;
	let mut memory = auth::load_ci_memory_store(
		root,
		args::repo_id_override(matches),
		matches.get_flag("require-auth"),
		args::explicit_token(matches),
	)?;

	let base_reasons = base_unconditional_reasons(&input);
	let graph = evaluate_graph_context(root, &input.changed_files, &input.settings)?;
	let on_main = is_main_branch(input.branch.as_deref(), input.git_ref.as_deref());
	let prepared = prepare_jobs(&input, &base_reasons, &graph, on_main, &memory);
	let phase = run_provider_phase(&input, &prepared, &memory, &graph).await;

	let mut records = Vec::with_capacity(prepared.len());

	for job in &prepared {
		let record = build_job_record(root, &input, job, &phase, &mut memory, on_main);
		memory.decisions.push(record.clone());
		records.push(record);
	}

	auth::save_ci_memory_store(root, &memory)?;

	let report = build_decide_report(&memory.repo_id, records);

	render_decide_report(&report, &input.format, input.github_output.as_deref())
}

/// Turn a pipeline error into the `--on-error run-all` fallback.
///
/// Prints a GitHub annotation so the failure is visible on the run, then emits
/// outputs that run every covered job and exits 0 — a failed gate job would
/// otherwise skip every dependent job, and a skipped required check counts as
/// passing. A failure of this fallback itself still exits non-zero.
fn fail_safe_decide(
	root: &Path,
	matches: &ArgMatches,
	github_output: Option<&Path>,
	error: &MonochangeError,
) -> MonochangeResult<String> {
	eprintln!("{FAIL_SAFE_ANNOTATION}{error}");

	let jobs = fail_safe_job_ids(root, matches);
	let outputs = render_fail_safe_outputs(&jobs)?;

	write_github_outputs(github_output, &outputs)?;

	Ok(outputs)
}

/// Job ids covered by the fallback: every configured job plus every `--job`.
///
/// Configuration loading is best effort here: when the config is what broke,
/// only the explicitly requested job ids are known.
fn fail_safe_job_ids(root: &Path, matches: &ArgMatches) -> Vec<String> {
	let mut jobs: BTreeSet<String> = args::collect_strings(matches, "job").into_iter().collect();

	if let Ok(settings) = load_ci_settings(root) {
		jobs.extend(settings.jobs.keys().cloned());
	}

	jobs.into_iter().collect()
}

/// Collect rule 1 and rule 3 state plus the graph verdict for every job.
fn prepare_jobs(
	input: &DecideInput,
	base_reasons: &[String],
	graph: &GraphContext,
	on_main: bool,
	memory: &CiMemoryStore,
) -> Vec<PreparedJob> {
	input
		.jobs
		.iter()
		.map(|id| {
			let job = input.settings.jobs.get(id);
			let (mut unconditional_reasons, matched_learned_rules, prior_skips) =
				unconditional_context(id, input, base_reasons, memory);
			let always = job.is_some_and(|job| job.always);
			let graph_verdict = evaluate_graph_decision_for_job(id, job, graph);

			if graph_verdict.source == VerdictSource::NoRule {
				eprintln!("warning: no ci rule for `{id}`; it will run unconditionally");
			}

			if on_main && prior_skips >= input.settings.max_skips_on_main {
				unconditional_reasons.push(format!(
					"consecutive skips on main ({prior_skips}) reached `[ci] max_skips_on_main` ({})",
					input.settings.max_skips_on_main
				));
			}

			PreparedJob {
				id: id.clone(),
				unconditional_reasons,
				matched_learned_rules,
				always,
				graph: graph_verdict,
			}
		})
		.collect()
}

/// Make at most one provider request covering every evaluated job.
async fn run_provider_phase(
	input: &DecideInput,
	prepared: &[PreparedJob],
	memory: &CiMemoryStore,
	graph: &GraphContext,
) -> ProviderPhase {
	if input.provider == CiDecisionProvider::Graph {
		return ProviderPhase::LocalGraph;
	}

	let needs_provider = prepared
		.iter()
		.any(|job| job.unconditional_reasons.is_empty() && !job.always);

	if !needs_provider {
		return ProviderPhase::LocalGraph;
	}

	let prompt_jobs: Vec<PromptJob<'_>> = prepared
		.iter()
		.map(|job| {
			PromptJob {
				id: job.id.as_str(),
				graph_run: job.graph.run,
				graph_reason: job.graph.reason.as_str(),
			}
		})
		.collect();
	let affected_packages = graph.affected_packages();
	let prompt = DecisionPrompt {
		jobs: &prompt_jobs,
		changed_files: &input.changed_files,
		affected_packages: &affected_packages,
		recent_history: &recent_history(memory),
	};

	match query_external_decision_provider(input.provider, &overrides_from(input), &prompt).await {
		Ok(outcome) => {
			ProviderPhase::Responded(outcome.decisions, outcome.provider_used, outcome.model_used)
		}
		Err(_) => ProviderPhase::Failed,
	}
}

/// Format the most recent decisions for a provider prompt.
fn recent_history(memory: &CiMemoryStore) -> Vec<String> {
	memory
		.decisions
		.iter()
		.rev()
		.take(RECENT_HISTORY_LIMIT)
		.map(|decision| format!("{}={}", decision.job, decision.decision))
		.collect()
}

/// Collect every unconditional override that applies to the whole invocation.
fn base_unconditional_reasons(input: &DecideInput) -> Vec<String> {
	let mut reasons = Vec::new();

	if input.unconditional_flag {
		reasons.push("explicit `--unconditional` flag".to_string());
	}

	extend_event_reasons(&mut reasons, input);
	extend_branch_reasons(&mut reasons, input);
	extend_label_reasons(&mut reasons, input);
	extend_commit_message_reasons(&mut reasons, input);
	extend_path_reasons(&mut reasons, input);

	reasons
}

/// Add a reason when the event always runs.
fn extend_event_reasons(reasons: &mut Vec<String>, input: &DecideInput) {
	let Some(event) = input.event.as_ref() else {
		return;
	};

	let matched = input
		.settings
		.unconditional_events
		.iter()
		.any(|candidate| candidate.eq_ignore_ascii_case(event));

	if matched {
		reasons.push(format!("event `{event}` matches unconditional events"));
	}
}

/// Add reasons when the branch or full ref always runs.
fn extend_branch_reasons(reasons: &mut Vec<String>, input: &DecideInput) {
	if let Some(branch) = input.branch.as_ref()
		&& let Some(matched) = matches_any_glob(branch, &input.settings.unconditional_branches)
	{
		reasons.push(format!(
			"branch `{branch}` matches unconditional branch pattern `{matched}`"
		));
	}

	if let Some(git_ref) = input.git_ref.as_ref()
		&& let Some(matched) = matches_any_glob(git_ref, &input.settings.unconditional_branches)
	{
		reasons.push(format!(
			"ref `{git_ref}` matches unconditional branch/ref pattern `{matched}`"
		));
	}
}

/// Add a reason for every label carrying an unconditional marker.
fn extend_label_reasons(reasons: &mut Vec<String>, input: &DecideInput) {
	for label in &input.labels {
		let lower = label.to_ascii_lowercase();

		if unconditional_marker(&lower).is_some() {
			reasons.push(format!("label `{label}` forces unconditional CI"));
		}
	}
}

/// Add a reason when the commit message carries an unconditional marker.
fn extend_commit_message_reasons(reasons: &mut Vec<String>, input: &DecideInput) {
	let Some(message) = input.commit_message.as_ref() else {
		return;
	};

	let lower = message.to_ascii_lowercase();

	if let Some(marker) = unconditional_marker(&lower) {
		reasons.push(format!(
			"commit message contains unconditional marker `{marker}`"
		));
	}
}

/// Add a reason for every changed path that always triggers a full run.
fn extend_path_reasons(reasons: &mut Vec<String>, input: &DecideInput) {
	for file in &input.changed_files {
		if let Some(matched) = matches_any_glob(file, &input.settings.unconditional_paths) {
			reasons.push(format!(
				"changed file `{file}` matches unconditional path pattern `{matched}`"
			));
		}
	}
}

/// Find the first unconditional marker in lowercase text.
fn unconditional_marker(text: &str) -> Option<&'static str> {
	UNCONDITIONAL_MARKERS
		.iter()
		.copied()
		.find(|marker| text.contains(marker))
}

/// Collect override reasons, learned-rule matches, and the prior skip count.
fn unconditional_context(
	job: &str,
	input: &DecideInput,
	base_reasons: &[String],
	memory: &CiMemoryStore,
) -> (Vec<String>, Vec<String>, u32) {
	let mut reasons = base_reasons.to_vec();
	let matched_rules = learned_rule_matches(job, &input.changed_files, memory, &mut reasons);
	let prior_skips = skip_count(memory, job);

	(reasons, matched_rules, prior_skips)
}

/// Record which learned rules matched this job and why they force a run.
fn learned_rule_matches(
	job: &str,
	changed_files: &[String],
	memory: &CiMemoryStore,
	reasons: &mut Vec<String>,
) -> Vec<String> {
	let mut matched_rules = Vec::new();

	for rule in &memory.learned_rules {
		if rule.job != "*" && rule.job != job {
			continue;
		}

		let matched_file = changed_files
			.iter()
			.find(|file| matches_any_glob(file, &rule.patterns).is_some());

		if let Some(file) = matched_file {
			matched_rules.push(rule.id.clone());
			reasons.push(format!(
				"learned rule `{}` (from past false_negative) matched `{file}`",
				rule.id
			));
		}
	}

	matched_rules
}

/// Read the stored consecutive-skip counter for a job.
fn skip_count(memory: &CiMemoryStore, job: &str) -> u32 {
	memory
		.consecutive_skips_on_main
		.get(job)
		.copied()
		.unwrap_or(0)
}

/// Update the consecutive-skip counter after a decision on `main`.
fn update_skip_counter(
	memory: &mut CiMemoryStore,
	job: &str,
	on_main: bool,
	run: bool,
	prior_skips: u32,
) -> u32 {
	if !on_main {
		return prior_skips;
	}

	if run {
		memory.consecutive_skips_on_main.insert(job.to_string(), 0);

		return 0;
	}

	let next = prior_skips + 1;

	memory
		.consecutive_skips_on_main
		.insert(job.to_string(), next);

	next
}

/// Evaluate one job end to end and persist its record into `memory`.
fn build_job_record(
	root: &Path,
	input: &DecideInput,
	job: &PreparedJob,
	phase: &ProviderPhase,
	memory: &mut CiMemoryStore,
	on_main: bool,
) -> CiDecisionRecord {
	let prior_skips = skip_count(memory, &job.id);
	let outcome = resolve_job_outcome(job, input, phase);
	let consecutive_skips_on_main =
		update_skip_counter(memory, &job.id, on_main, outcome.run, prior_skips);
	let duration = analyze_duration(root, &job.id, input, memory);
	let sequence = memory.decisions.len() + 1;
	let unconditional = !job.unconditional_reasons.is_empty();

	CiDecisionRecord {
		id: format!("dec-{sequence}-{}", unix_millis()),
		repo_id: memory.repo_id.clone(),
		job: job.id.clone(),
		decision: decision_label(outcome.run).to_string(),
		run: outcome.run,
		unconditional,
		provider: outcome.provider,
		model: outcome.model,
		reason: outcome.reason,
		unconditional_reasons: job.unconditional_reasons.clone(),
		matched_learned_rules: job.matched_learned_rules.clone(),
		affected_packages: job.graph.affected_packages.clone(),
		changed_files: input.changed_files.clone(),
		event: input.event.clone(),
		branch: input.branch.clone().or_else(|| input.git_ref.clone()),
		commit: input.commit.clone(),
		consecutive_skips_on_main,
		duration_red_flag: duration.red_flag,
		observed_duration_seconds: duration.observed_seconds,
		threshold_minutes: duration.threshold_minutes,
		speed_suggestions: duration.suggestions,
		timestamp: Utc::now().to_rfc3339(),
	}
}

/// Resolve the final run/reason/provider/model for a job.
fn resolve_job_outcome(
	job: &PreparedJob,
	input: &DecideInput,
	phase: &ProviderPhase,
) -> JobOutcome {
	if !job.unconditional_reasons.is_empty() {
		return JobOutcome {
			run: true,
			reason: format!(
				"forced unconditional run (`y`): {}",
				job.unconditional_reasons.join("; ")
			),
			provider: input.provider.as_str().to_string(),
			model: configured_model(input),
		};
	}

	if job.always {
		return JobOutcome {
			run: true,
			reason: "`always = true` in `[ci.jobs]`".to_string(),
			provider: input.provider.as_str().to_string(),
			model: configured_model(input),
		};
	}

	match phase {
		ProviderPhase::LocalGraph => graph_outcome(input, &job.graph, ""),
		ProviderPhase::Failed => graph_outcome(input, &job.graph, GRAPH_FALLBACK_SUFFIX),
		ProviderPhase::Responded(decisions, provider_used, model_used) => {
			match decisions.get(&job.id) {
				None => {
					let mut outcome = graph_outcome(input, &job.graph, "");
					outcome
						.reason
						.push_str("; provider response omitted this job");
					outcome.provider.clone_from(provider_used);
					outcome.model.clone_from(model_used);
					outcome
				}
				Some(decision) => {
					provider_decision_outcome(job, decision, input, provider_used, model_used)
				}
			}
		}
	}
}

/// Merge one provider decision with the graph verdict.
fn provider_decision_outcome(
	job: &PreparedJob,
	decision: &ProviderDecision,
	input: &DecideInput,
	provider_used: &str,
	model_used: &str,
) -> JobOutcome {
	if decision.run {
		return JobOutcome {
			run: true,
			reason: decision.reason.clone(),
			provider: provider_used.to_string(),
			model: model_used.to_string(),
		};
	}

	if job.graph.run && !input.settings.ai_can_skip {
		return JobOutcome {
			run: true,
			reason: format!(
				"provider skip ignored (`ai_can_skip = false`); graph verdict: {}",
				job.graph.reason
			),
			provider: provider_used.to_string(),
			model: model_used.to_string(),
		};
	}

	JobOutcome {
		run: false,
		reason: decision.reason.clone(),
		provider: provider_used.to_string(),
		model: model_used.to_string(),
	}
}

/// Outcome based on the deterministic graph verdict.
fn graph_outcome(input: &DecideInput, graph: &GraphVerdict, suffix: &str) -> JobOutcome {
	JobOutcome {
		run: graph.run,
		reason: format!("{}{suffix}", graph.reason),
		provider: GRAPH_PROVIDER_LABEL.to_string(),
		model: configured_model(input),
	}
}

/// Model override when supplied, otherwise the provider's default model.
fn configured_model(input: &DecideInput) -> String {
	input
		.model_override
		.clone()
		.or_else(|| input.settings.model.clone())
		.unwrap_or_else(|| default_model(input.provider).to_string())
}

/// Build provider overrides from the parsed input.
fn overrides_from(input: &DecideInput) -> ProviderOverrides<'_> {
	ProviderOverrides {
		model: input.model_override.as_deref(),
		api_url: input.api_url_override.as_deref(),
		api_key: input.api_key_override.as_deref(),
		account_id: input.account_id_override.as_deref(),
	}
}

/// Default model identifier for a provider.
fn default_model(provider: CiDecisionProvider) -> &'static str {
	match provider {
		CiDecisionProvider::Graph => GRAPH_MODEL_LABEL,
		CiDecisionProvider::Jev => super::provider::JEV_MODEL_LABEL,
		other => default_model_for(other).expect("external providers have an endpoint preset"),
	}
}

/// Analyse observed or historical duration against the job's threshold.
fn analyze_duration(
	root: &Path,
	job: &str,
	input: &DecideInput,
	memory: &CiMemoryStore,
) -> DurationAnalysis {
	let is_rust = is_rust_job_or_context(job, &input.changed_files, root);
	let threshold_minutes = duration_threshold_minutes(
		is_rust,
		input.max_duration_minutes,
		input.rust_max_duration_minutes,
	);
	let observed_seconds = input
		.explicit_duration
		.or_else(|| historical_max_duration(memory, job));
	let red_flag = observed_seconds
		.is_some_and(|seconds| exceeds_duration_threshold(seconds, threshold_minutes));
	let suggestions =
		optional_duration_suggestions(job, is_rust, observed_seconds, threshold_minutes);

	DurationAnalysis {
		red_flag,
		observed_seconds,
		threshold_minutes,
		suggestions,
	}
}

/// Longest verification duration recorded for a job.
fn historical_max_duration(memory: &CiMemoryStore, job: &str) -> Option<u64> {
	memory
		.verifications
		.iter()
		.filter(|verification| verification.job == *job)
		.map(|verification| verification.duration_seconds)
		.max()
}

/// Build the aggregate report, using the first job as the headline decision.
fn build_decide_report(repo_id: &str, records: Vec<CiDecisionRecord>) -> CiDecideReport {
	let overall_run = records.iter().any(|record| record.run);
	let overall_unconditional = records.iter().any(|record| record.unconditional);
	let primary = records
		.first()
		.cloned()
		.unwrap_or_else(|| panic!("`ci decide` always evaluates at least one job"));

	CiDecideReport {
		repo_id: repo_id.to_string(),
		decision: decision_label(overall_run).to_string(),
		run: overall_run,
		unconditional: overall_unconditional,
		provider: primary.provider,
		model: primary.model,
		reason: primary.reason,
		jobs: records,
	}
}

/// CLI decision label for a boolean verdict.
fn decision_label(run: bool) -> &'static str {
	if run { "y" } else { "n" }
}

/// Milliseconds since the Unix epoch, used to keep record ids unique.
fn unix_millis() -> u128 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map_or(0, |duration| duration.as_millis())
}
