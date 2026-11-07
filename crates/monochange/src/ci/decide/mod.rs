//! The `ci decide` subcommand.
//!
//! Deciding is a pipeline of small steps: collect the CLI/environment context,
//! derive unconditional overrides, ask the graph (or a provider), update the
//! skip counter, and record the decision. Keeping each step separate makes the
//! reason string auditable, which is the part CI operators actually read.

mod input;

use std::path::Path;

use chrono::Utc;
use clap::ArgMatches;
use input::DecideInput;
use input::parse_decide_input;
use monochange_core::MonochangeResult;

use super::DEFAULT_CLOUDFLARE_MODEL;
use super::DEFAULT_OPENCODE_GO_MODEL;
use super::DEFAULT_OPENROUTER_MODEL;
use super::UNCONDITIONAL_MARKERS;
use super::args;
use super::auth;
use super::graph::WorkspaceGraphContext;
use super::graph::duration_threshold_minutes;
use super::graph::evaluate_graph_decision_for_job;
use super::graph::evaluate_workspace_graph_context;
use super::graph::exceeds_duration_threshold;
use super::graph::is_main_branch;
use super::graph::is_rust_job_or_context;
use super::graph::matches_any_glob;
use super::graph::optional_duration_suggestions;
use super::provider::DecisionPrompt;
use super::provider::ProviderOverrides;
use super::provider::query_external_decision_provider;
use super::render::render_decide_report;
use super::types::CiDecideReport;
use super::types::CiDecisionProvider;
use super::types::CiDecisionRecord;
use super::types::CiMemoryStore;

/// Provider and model used when the job name does not select one.
const GRAPH_PROVIDER_LABEL: &str = "graph";
/// Model label recorded for the deterministic workspace graph.
const GRAPH_MODEL_LABEL: &str = "workspace-graph";
/// Model label recorded for the JEV webhook provider.
const JEV_MODEL_LABEL: &str = "jev-webhook";
/// Suffix appended when an external provider fails and the graph takes over.
const GRAPH_FALLBACK_SUFFIX: &str = " (evaluated via deterministic graph fallback)";

/// Provider verdict for a single job.
struct JobOutcome {
	run: bool,
	reason: String,
	provider: String,
	model: String,
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
	let input = parse_decide_input(root, matches)?;
	let mut memory = auth::load_ci_memory_store(
		root,
		args::repo_id_override(matches),
		matches.get_flag("require-auth"),
		args::explicit_token(matches),
	)?;

	let base_reasons = base_unconditional_reasons(&input);
	let graph = evaluate_workspace_graph_context(root, &input.changed_files);
	let on_main = is_main_branch(input.branch.as_deref(), input.git_ref.as_deref());
	let mut records = Vec::with_capacity(input.jobs.len());

	for job in &input.jobs {
		let record = evaluate_job_decision(
			root,
			job,
			&input,
			&base_reasons,
			&graph,
			on_main,
			&mut memory,
		)
		.await;

		memory.decisions.push(record.clone());
		records.push(record);
	}

	auth::save_ci_memory_store(root, &memory)?;

	let report = build_decide_report(&memory.repo_id, records);

	render_decide_report(&report, &input.format)
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
		&& let Some(matched) = matches_any_glob(branch, &input.unconditional_branches)
	{
		reasons.push(format!(
			"branch `{branch}` matches unconditional branch pattern `{matched}`"
		));
	}

	if let Some(git_ref) = input.git_ref.as_ref()
		&& let Some(matched) = matches_any_glob(git_ref, &input.unconditional_branches)
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
		if let Some(matched) = matches_any_glob(file, &input.unconditional_paths) {
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
	on_main: bool,
	memory: &CiMemoryStore,
) -> (Vec<String>, Vec<String>, u32) {
	let mut reasons = base_reasons.to_vec();
	let matched_rules = learned_rule_matches(job, &input.changed_files, memory, &mut reasons);
	let prior_skips = skip_count(memory, job);

	if on_main && prior_skips >= input.max_skips_on_main {
		reasons.push(format!(
			"consecutive skips on main ({prior_skips}) reached `--max-skips-on-main` ({})",
			input.max_skips_on_main
		));
	}

	(reasons, matched_rules, prior_skips)
}

/// Fields that vary per job when assembling the stored decision record.
struct DecisionRecordParts {
	run: bool,
	unconditional: bool,
	unconditional_reasons: Vec<String>,
	matched_learned_rules: Vec<String>,
	provider: String,
	model: String,
	reason: String,
	affected_packages: Vec<String>,
	consecutive_skips_on_main: u32,
	duration: DurationAnalysis,
}

/// Evaluate one job, updating the shared memory store as a side effect.
async fn evaluate_job_decision(
	root: &Path,
	job: &str,
	input: &DecideInput,
	base_reasons: &[String],
	graph: &WorkspaceGraphContext,
	on_main: bool,
	memory: &mut CiMemoryStore,
) -> CiDecisionRecord {
	let (unconditional_reasons, matched_learned_rules, prior_skips) =
		unconditional_context(job, input, base_reasons, on_main, memory);
	let unconditional = !unconditional_reasons.is_empty();
	let (graph_run, graph_reason, affected_packages) = evaluate_graph_decision_for_job(
		job,
		&input.changed_files,
		&input.job_paths,
		&input.job_packages,
		graph,
	);
	let graph_inputs = GraphInputs {
		run: graph_run,
		reason: &graph_reason,
		affected_packages: &affected_packages,
	};
	let outcome = resolve_job_outcome(
		job,
		input,
		unconditional,
		&unconditional_reasons,
		&graph_inputs,
	)
	.await;
	let consecutive_skips_on_main =
		update_skip_counter(memory, job, on_main, outcome.run, prior_skips);
	let duration = analyze_duration(root, job, input, memory);

	build_decision_record(
		memory,
		input,
		job,
		DecisionRecordParts {
			run: outcome.run,
			unconditional,
			unconditional_reasons,
			matched_learned_rules,
			provider: outcome.provider,
			model: outcome.model,
			reason: outcome.reason,
			affected_packages,
			consecutive_skips_on_main,
			duration,
		},
	)
}

/// Assemble the stored decision record for one job.
fn build_decision_record(
	memory: &CiMemoryStore,
	input: &DecideInput,
	job: &str,
	parts: DecisionRecordParts,
) -> CiDecisionRecord {
	let sequence = memory.decisions.len() + 1;

	CiDecisionRecord {
		id: format!("dec-{sequence}-{}", unix_millis()),
		repo_id: memory.repo_id.clone(),
		job: job.to_string(),
		decision: decision_label(parts.run).to_string(),
		run: parts.run,
		unconditional: parts.unconditional,
		provider: parts.provider,
		model: parts.model,
		reason: parts.reason,
		unconditional_reasons: parts.unconditional_reasons,
		matched_learned_rules: parts.matched_learned_rules,
		affected_packages: parts.affected_packages,
		changed_files: input.changed_files.clone(),
		event: input.event.clone(),
		branch: input.branch.clone().or_else(|| input.git_ref.clone()),
		commit: input.commit.clone(),
		consecutive_skips_on_main: parts.consecutive_skips_on_main,
		duration_red_flag: parts.duration.red_flag,
		observed_duration_seconds: parts.duration.observed_seconds,
		threshold_minutes: parts.duration.threshold_minutes,
		speed_suggestions: parts.duration.suggestions,
		timestamp: Utc::now().to_rfc3339(),
	}
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

/// Graph verdict borrowed for provider fallback decisions.
struct GraphInputs<'a> {
	run: bool,
	reason: &'a str,
	affected_packages: &'a [String],
}

/// Resolve the final run/reason/provider/model for a job.
async fn resolve_job_outcome(
	job: &str,
	input: &DecideInput,
	unconditional: bool,
	unconditional_reasons: &[String],
	graph_inputs: &GraphInputs<'_>,
) -> JobOutcome {
	if unconditional {
		return forced_outcome(input, unconditional_reasons);
	}

	if input.provider == CiDecisionProvider::Graph {
		return graph_outcome(input, graph_inputs);
	}

	let provider_result = query_external_decision_provider(
		input.provider,
		&overrides_from(input),
		&prompt_from(input, job, graph_inputs.affected_packages),
	)
	.await;

	match provider_result {
		Ok(outcome) => provider_outcome(outcome),
		Err(_) => fallback_outcome(input, graph_inputs),
	}
}

/// Outcome for a job forced to run by an override.
fn forced_outcome(input: &DecideInput, unconditional_reasons: &[String]) -> JobOutcome {
	JobOutcome {
		run: true,
		reason: format!(
			"forced unconditional run (`y`): {}",
			unconditional_reasons.join("; ")
		),
		provider: input.provider.as_str().to_string(),
		model: configured_model(input),
	}
}

/// Outcome for the deterministic workspace graph provider.
fn graph_outcome(input: &DecideInput, graph_inputs: &GraphInputs<'_>) -> JobOutcome {
	JobOutcome {
		run: graph_inputs.run,
		reason: graph_inputs.reason.to_string(),
		provider: GRAPH_PROVIDER_LABEL.to_string(),
		model: configured_model(input),
	}
}

/// Outcome reported by an external provider.
fn provider_outcome(outcome: super::provider::ProviderCallOutcome) -> JobOutcome {
	JobOutcome {
		run: outcome.run,
		reason: outcome.reason,
		provider: outcome.provider_used,
		model: outcome.model_used,
	}
}

/// Outcome used when an external provider fails and the graph takes over.
fn fallback_outcome(input: &DecideInput, graph_inputs: &GraphInputs<'_>) -> JobOutcome {
	JobOutcome {
		run: graph_inputs.run,
		reason: format!("{}{GRAPH_FALLBACK_SUFFIX}", graph_inputs.reason),
		provider: GRAPH_PROVIDER_LABEL.to_string(),
		model: configured_model(input),
	}
}

/// Model override when supplied, otherwise the provider's default model.
fn configured_model(input: &DecideInput) -> String {
	input
		.model_override
		.clone()
		.unwrap_or_else(|| default_model(input.provider))
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

/// Build the provider prompt for one job.
fn prompt_from<'a>(
	input: &'a DecideInput,
	job: &'a str,
	affected_packages: &'a [String],
) -> DecisionPrompt<'a> {
	DecisionPrompt {
		job,
		changed_files: &input.changed_files,
		affected_packages,
	}
}

/// Default model identifier for a provider.
fn default_model(provider: CiDecisionProvider) -> String {
	let model = match provider {
		CiDecisionProvider::OpencodeGo | CiDecisionProvider::OpenaiCompatible => {
			DEFAULT_OPENCODE_GO_MODEL
		}
		CiDecisionProvider::Openrouter => DEFAULT_OPENROUTER_MODEL,
		CiDecisionProvider::Cloudflare => DEFAULT_CLOUDFLARE_MODEL,
		CiDecisionProvider::Jev => JEV_MODEL_LABEL,
		CiDecisionProvider::Graph => GRAPH_MODEL_LABEL,
	};

	model.to_string()
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
		.expect("`ci decide` always evaluates at least one job");

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
