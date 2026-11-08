//! The `ci verify` subcommand.
//!
//! Verification closes the loop: it compares a recorded decision against the
//! real CI outcome and classifies the pair. A false negative is the expensive
//! case, so it immediately writes a learned rule that forces future runs on the
//! same paths.

use std::path::Path;

use chrono::Utc;
use clap::ArgMatches;
use monochange_config::load_ci_settings;
use monochange_core::MonochangeResult;

use super::DEFAULT_MAX_DURATION_MINUTES;
use super::DEFAULT_RUST_MAX_DURATION_MINUTES;
use super::args;
use super::auth;
use super::graph::GraphContext;
use super::graph::derive_learned_patterns_from_files;
use super::graph::duration_threshold_minutes;
use super::graph::evaluate_graph_context;
use super::graph::evaluate_graph_decision_for_job;
use super::graph::exceeds_duration_threshold;
use super::graph::is_main_branch;
use super::graph::is_rust_job_or_context;
use super::graph::optional_duration_suggestions;
use super::render::render_verification_report;
use super::types::CiLearnedRule;
use super::types::CiMemoryStore;
use super::types::CiVerificationClassification;
use super::types::CiVerificationRecord;

/// Classification string used when the gate skipped the job.
const SKIP_DECISION: &str = "n";
/// Classification string used when the gate ran the job.
const RUN_DECISION: &str = "y";
/// Conclusion that indicates the job failed.
const FAILURE_CONCLUSION: &str = "failure";
/// Conclusion that indicates the job passed.
const SUCCESS_CONCLUSION: &str = "success";
/// Default branch recorded when `--branch` is omitted.
const DEFAULT_BRANCH: &str = "main";

/// Parsed arguments for one verification.
struct VerifyInput {
	job: String,
	conclusion: String,
	duration_seconds: u64,
	commit: Option<String>,
	pr: Option<String>,
	branch: Option<String>,
	explicit_prior: Option<String>,
	explicit_redundant: bool,
	changed_files: Vec<String>,
	max_duration_minutes: u64,
	rust_max_duration_minutes: u64,
	format: String,
}

/// Graph evidence gathered before classifying the verification.
struct VerifyContext<'a> {
	input: &'a VerifyInput,
	changed_files: &'a [String],
	graph: &'a GraphContext,
	graph_would_run: bool,
	affected_packages: Vec<String>,
}

/// Classify one job outcome against its gate decision.
pub(super) fn run_ci_verify_subcommand(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	let input = parse_verify_input(matches)?;
	let mut memory = auth::load_ci_memory_store(
		root,
		args::repo_id_override(matches),
		matches.get_flag("require-auth"),
		args::explicit_token(matches),
	)?;
	let prior_record = find_prior_decision(&memory, &input);
	let changed_files = resolve_verify_changed_files(&input, prior_record.as_ref());
	let settings = load_ci_settings(root)?;
	let graph = evaluate_graph_context(root, &changed_files, &settings)?;
	let job_rule = settings.jobs.get(&input.job);
	let graph_verdict = evaluate_graph_decision_for_job(&input.job, job_rule, &graph);
	let context = VerifyContext {
		input: &input,
		changed_files: &changed_files,
		graph: &graph,
		graph_would_run: graph_verdict.run,
		affected_packages: graph_verdict.affected_packages,
	};

	let prior_decision = resolve_prior_decision(prior_record.as_ref(), &context);
	let (classification, reason) = classify_verification(&prior_decision, &context);
	let learned_rule_added = learned_rule_for(&mut memory, classification, &context);
	let duration = verify_duration_analysis(root, &context);

	update_main_skip_counter(&mut memory, &context);

	let verification = build_verification_record(
		&memory,
		&context,
		prior_decision,
		classification,
		reason,
		learned_rule_added,
		duration,
	);

	memory.verifications.push(verification.clone());
	auth::save_ci_memory_store(root, &memory)?;

	render_verification_report(&verification, &input.format)
}

/// Parse CLI arguments into `VerifyInput`.
fn parse_verify_input(matches: &ArgMatches) -> MonochangeResult<VerifyInput> {
	Ok(VerifyInput {
		job: args::required_arg(matches, "job")?,
		conclusion: args::required_arg(matches, "conclusion")?,
		duration_seconds: matches
			.get_one::<u64>("duration-seconds")
			.copied()
			.unwrap_or_default(),
		commit: matches.get_one::<String>("commit").cloned(),
		pr: matches.get_one::<String>("pr").cloned(),
		branch: matches
			.get_one::<String>("branch")
			.cloned()
			.or_else(|| Some(DEFAULT_BRANCH.to_string())),
		explicit_prior: matches.get_one::<String>("prior-decision").cloned(),
		explicit_redundant: matches.get_flag("redundant"),
		changed_files: args::collect_normalized_paths(matches, "changed-files"),
		max_duration_minutes: args::duration_minutes_arg(
			matches,
			"max-duration-minutes",
			DEFAULT_MAX_DURATION_MINUTES,
		),
		rust_max_duration_minutes: args::duration_minutes_arg(
			matches,
			"rust-max-duration-minutes",
			DEFAULT_RUST_MAX_DURATION_MINUTES,
		),
		format: args::format_arg(matches).to_string(),
	})
}

/// Find the most recent decision for this job, preferring a commit match.
fn find_prior_decision(
	memory: &CiMemoryStore,
	input: &VerifyInput,
) -> Option<super::types::CiDecisionRecord> {
	memory
		.decisions
		.iter()
		.rev()
		.find(|decision| {
			decision.job == input.job
				&& (input.commit.is_none() || decision.commit.as_deref() == input.commit.as_deref())
		})
		.or_else(|| {
			memory
				.decisions
				.iter()
				.rev()
				.find(|decision| decision.job == input.job)
		})
		.cloned()
}

/// Use explicit changed files, then the prior decision's files, then nothing.
fn resolve_verify_changed_files(
	input: &VerifyInput,
	prior_record: Option<&super::types::CiDecisionRecord>,
) -> Vec<String> {
	if !input.changed_files.is_empty() {
		return input.changed_files.clone();
	}

	prior_record
		.map(|prior| prior.changed_files.clone())
		.unwrap_or_default()
}

/// Work out which decision the verification is being compared against.
fn resolve_prior_decision(
	prior_record: Option<&super::types::CiDecisionRecord>,
	context: &VerifyContext<'_>,
) -> String {
	if let Some(explicit_prior) = context.input.explicit_prior.as_deref() {
		return explicit_prior_label(explicit_prior).to_string();
	}

	if let Some(prior) = prior_record {
		let forced_failure = prior.unconditional
			&& !context.graph_would_run
			&& context.input.conclusion == FAILURE_CONCLUSION;

		if forced_failure {
			return SKIP_DECISION.to_string();
		}

		return prior.decision.clone();
	}

	if context.graph_would_run {
		return RUN_DECISION.to_string();
	}

	SKIP_DECISION.to_string()
}

/// Map an explicit `--prior-decision` alias onto `y` or `n`.
fn explicit_prior_label(value: &str) -> &'static str {
	if matches!(value, "y" | "true" | "run") {
		RUN_DECISION
	} else {
		SKIP_DECISION
	}
}

/// Classify the decision/outcome pair and explain the result.
fn classify_verification(
	prior_decision: &str,
	context: &VerifyContext<'_>,
) -> (CiVerificationClassification, String) {
	let job = &context.input.job;
	let is_failure = context.input.conclusion == FAILURE_CONCLUSION;

	match (prior_decision, is_failure) {
		(SKIP_DECISION, true) => {
			(
				CiVerificationClassification::FalseNegative,
				format!(
					"job `{job}` was skipped (`n`) or would have been skipped, but failed on verification (`{}`); adding learned override rule",
					context.input.conclusion
				),
			)
		}
		(SKIP_DECISION, false) => {
			(
				CiVerificationClassification::TrueNegative,
				format!(
					"job `{job}` was safely skipped (`n`) and verified `{}` on {}",
					context.input.conclusion,
					context.input.branch.as_deref().unwrap_or(DEFAULT_BRANCH)
				),
			)
		}
		(_, true) => {
			(
				CiVerificationClassification::TruePositive,
				format!("job `{job}` ran (`y`) and caught a failure"),
			)
		}
		(_, false) => passed_verification(job, context),
	}
}

/// Classify a passing run as either a wasted run or a useful one.
fn passed_verification(
	job: &str,
	context: &VerifyContext<'_>,
) -> (CiVerificationClassification, String) {
	let unaffected_and_ignored = !context.changed_files.is_empty()
		&& (context.graph.ignored_only() || !context.graph_would_run);

	if context.input.explicit_redundant || unaffected_and_ignored {
		return (
			CiVerificationClassification::FalsePositive,
			format!(
				"job `{job}` ran (`y`) and passed, but changed files did not affect its packages or only touched non-code/unrelated paths"
			),
		);
	}

	(
		CiVerificationClassification::TruePositive,
		format!(
			"job `{job}` ran (`y`) and verified affected changes with `{}`",
			context.input.conclusion
		),
	)
}

/// Learn an override rule when a skipped job later failed.
fn learned_rule_for(
	memory: &mut CiMemoryStore,
	classification: CiVerificationClassification,
	context: &VerifyContext<'_>,
) -> Option<CiLearnedRule> {
	if classification != CiVerificationClassification::FalseNegative {
		return None;
	}

	let job = &context.input.job;
	let patterns = derive_learned_patterns_from_files(context.changed_files);

	if let Some(existing) = memory
		.learned_rules
		.iter()
		.find(|rule| rule.job == *job && rule.patterns == patterns)
	{
		return Some(existing.clone());
	}

	let rule = new_learned_rule(memory, context, patterns);

	memory.learned_rules.push(rule.clone());

	Some(rule)
}

/// Build a learned rule that forces the job to run on the failing paths.
fn new_learned_rule(
	memory: &CiMemoryStore,
	context: &VerifyContext<'_>,
	patterns: Vec<String>,
) -> CiLearnedRule {
	let commit_suffix = context
		.input
		.commit
		.as_deref()
		.map(|commit| format!(" at commit {commit}"))
		.unwrap_or_default();

	CiLearnedRule {
		id: format!("rule-{}-{}", memory.learned_rules.len() + 1, unix_millis()),
		job: context.input.job.clone(),
		patterns,
		packages: context.affected_packages.clone(),
		reason: format!(
			"Auto-learned from false_negative on `{}`{commit_suffix}",
			context.input.job
		),
		source_commit: context.input.commit.clone(),
		created_at: Utc::now().to_rfc3339(),
	}
}

/// Reset the `main` skip counter after a successful verification.
fn update_main_skip_counter(memory: &mut CiMemoryStore, context: &VerifyContext<'_>) {
	let branch = context.input.branch.as_deref();
	let is_success = context.input.conclusion == SUCCESS_CONCLUSION;

	if !is_main_branch(branch, None) || !is_success {
		return;
	}

	memory
		.consecutive_skips_on_main
		.insert(context.input.job.clone(), 0);
}

/// Duration red-flag details for the verification record.
struct VerifyDuration {
	red_flag: bool,
	threshold_minutes: u64,
	suggestions: Vec<String>,
}

/// Evaluate the actual job duration against the threshold for this job type.
fn verify_duration_analysis(root: &Path, context: &VerifyContext<'_>) -> VerifyDuration {
	let input = context.input;
	let is_rust = is_rust_job_or_context(&input.job, context.changed_files, root);
	let threshold_minutes = duration_threshold_minutes(
		is_rust,
		input.max_duration_minutes,
		input.rust_max_duration_minutes,
	);
	let red_flag = exceeds_duration_threshold(input.duration_seconds, threshold_minutes);
	let suggestions = optional_duration_suggestions(
		&input.job,
		is_rust,
		Some(input.duration_seconds),
		threshold_minutes,
	);

	VerifyDuration {
		red_flag,
		threshold_minutes,
		suggestions,
	}
}

/// Assemble the stored verification record.
fn build_verification_record(
	memory: &CiMemoryStore,
	context: &VerifyContext<'_>,
	prior_decision: String,
	classification: CiVerificationClassification,
	reason: String,
	learned_rule_added: Option<CiLearnedRule>,
	duration: VerifyDuration,
) -> CiVerificationRecord {
	let input = context.input;

	CiVerificationRecord {
		id: format!("ver-{}-{}", memory.verifications.len() + 1, unix_millis()),
		repo_id: memory.repo_id.clone(),
		job: input.job.clone(),
		conclusion: input.conclusion.clone(),
		duration_seconds: input.duration_seconds,
		commit: input.commit.clone(),
		pr: input.pr.clone(),
		branch: input.branch.clone(),
		prior_decision,
		classification,
		reason,
		changed_files: context.changed_files.to_vec(),
		affected_packages: context.affected_packages.clone(),
		learned_rule_added,
		duration_red_flag: duration.red_flag,
		threshold_minutes: duration.threshold_minutes,
		speed_suggestions: duration.suggestions,
		timestamp: Utc::now().to_rfc3339(),
	}
}

/// Milliseconds since the Unix epoch, used to keep record ids unique.
fn unix_millis() -> u128 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map_or(0, |duration| duration.as_millis())
}
