//! CLI and environment parsing for `ci decide`.
//!
//! The parsed input is a flat snapshot of everything the decision pipeline
//! needs, so the evaluator never reaches back into `clap` matches or the
//! environment while it is running.

use std::path::Path;

use clap::ArgMatches;
use monochange_core::MonochangeResult;

use super::super::DEFAULT_MAX_DURATION_MINUTES;
use super::super::DEFAULT_MAX_SKIPS_ON_MAIN;
use super::super::DEFAULT_RUST_MAX_DURATION_MINUTES;
use super::super::DEFAULT_UNCONDITIONAL_BRANCHES;
use super::super::DEFAULT_UNCONDITIONAL_EVENTS;
use super::super::DEFAULT_UNCONDITIONAL_PATHS;
use super::super::args;
use super::super::graph::detect_changed_files_from_git;
use super::super::types::CiDecisionProvider;
/// Everything `ci decide` needs once CLI arguments and environment are resolved.
pub(super) struct DecideInput {
	pub(super) jobs: Vec<String>,
	pub(super) provider: CiDecisionProvider,
	pub(super) model_override: Option<String>,
	pub(super) api_url_override: Option<String>,
	pub(super) api_key_override: Option<String>,
	pub(super) account_id_override: Option<String>,
	pub(super) changed_files: Vec<String>,
	pub(super) event: Option<String>,
	pub(super) branch: Option<String>,
	pub(super) git_ref: Option<String>,
	pub(super) commit: Option<String>,
	pub(super) labels: Vec<String>,
	pub(super) commit_message: Option<String>,
	pub(super) unconditional_flag: bool,
	pub(super) unconditional_events: Vec<String>,
	pub(super) unconditional_branches: Vec<String>,
	pub(super) unconditional_paths: Vec<String>,
	pub(super) max_skips_on_main: u32,
	pub(super) explicit_duration: Option<u64>,
	pub(super) max_duration_minutes: u64,
	pub(super) rust_max_duration_minutes: u64,
	pub(super) job_paths: Vec<String>,
	pub(super) job_packages: Vec<String>,
	pub(super) format: String,
}

/// Parse CLI arguments and environment into `DecideInput`.
pub(super) fn parse_decide_input(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<DecideInput> {
	let provider_name = matches
		.get_one::<String>("provider")
		.map_or("opencode-go", String::as_str);
	let mut input = DecideInput::new(CiDecisionProvider::parse(provider_name)?);

	input.jobs = collect_jobs(matches);
	input.changed_files = collect_changed_files(root, matches);

	fill_provider_settings(&mut input, matches);
	fill_invocation_context(&mut input, matches);
	fill_unconditional_settings(&mut input, matches);
	fill_limits(&mut input, matches);
	fill_scope(&mut input, matches);

	Ok(input)
}

impl DecideInput {
	/// Start from empty settings for the selected provider.
	fn new(provider: CiDecisionProvider) -> Self {
		Self {
			jobs: Vec::new(),
			provider,
			model_override: None,
			api_url_override: None,
			api_key_override: None,
			account_id_override: None,
			changed_files: Vec::new(),
			event: None,
			branch: None,
			git_ref: None,
			commit: None,
			labels: Vec::new(),
			commit_message: None,
			unconditional_flag: false,
			unconditional_events: Vec::new(),
			unconditional_branches: Vec::new(),
			unconditional_paths: Vec::new(),
			max_skips_on_main: DEFAULT_MAX_SKIPS_ON_MAIN,
			explicit_duration: None,
			max_duration_minutes: DEFAULT_MAX_DURATION_MINUTES,
			rust_max_duration_minutes: DEFAULT_RUST_MAX_DURATION_MINUTES,
			job_paths: Vec::new(),
			job_packages: Vec::new(),
			format: "text".to_string(),
		}
	}
}

/// Provider endpoint and model overrides.
fn fill_provider_settings(input: &mut DecideInput, matches: &ArgMatches) {
	input.model_override = matches.get_one::<String>("model").cloned();
	input.api_url_override = matches.get_one::<String>("api-url").cloned();
	input.api_key_override = matches.get_one::<String>("api-key").cloned();
	input.account_id_override = matches.get_one::<String>("account-id").cloned();
}

/// Event, branch, commit, and label context.
fn fill_invocation_context(input: &mut DecideInput, matches: &ArgMatches) {
	input.event = args::cli_or_env(matches, "event", "GITHUB_EVENT_NAME");
	input.branch = args::cli_or_env(matches, "branch", "GITHUB_REF_NAME");
	input.git_ref = args::cli_or_env(matches, "ref", "GITHUB_REF");
	input.commit = args::cli_or_env(matches, "commit", "GITHUB_SHA");
	input.labels = args::collect_strings(matches, "labels");
	input.commit_message = matches.get_one::<String>("commit-message").cloned();
}

/// Flags and globs that force a full run.
fn fill_unconditional_settings(input: &mut DecideInput, matches: &ArgMatches) {
	input.unconditional_flag = matches.get_flag("unconditional");
	input.unconditional_events = args::collect_patterns(
		matches,
		"unconditional-events",
		DEFAULT_UNCONDITIONAL_EVENTS,
	);
	input.unconditional_branches = args::collect_patterns(
		matches,
		"unconditional-branches",
		DEFAULT_UNCONDITIONAL_BRANCHES,
	);
	input.unconditional_paths =
		args::collect_patterns(matches, "unconditional-paths", DEFAULT_UNCONDITIONAL_PATHS);
}

/// Skip counters and duration thresholds.
fn fill_limits(input: &mut DecideInput, matches: &ArgMatches) {
	input.max_skips_on_main = matches
		.get_one::<u32>("max-skips-on-main")
		.copied()
		.unwrap_or(DEFAULT_MAX_SKIPS_ON_MAIN);
	input.explicit_duration = matches.get_one::<u64>("duration-seconds").copied();
	input.max_duration_minutes = args::duration_minutes_arg(
		matches,
		"max-duration-minutes",
		DEFAULT_MAX_DURATION_MINUTES,
	);
	input.rust_max_duration_minutes = args::duration_minutes_arg(
		matches,
		"rust-max-duration-minutes",
		DEFAULT_RUST_MAX_DURATION_MINUTES,
	);
}

/// Explicit job scope and output format.
fn fill_scope(input: &mut DecideInput, matches: &ArgMatches) {
	input.job_paths = args::collect_strings(matches, "job-paths");
	input.job_packages = args::collect_strings(matches, "job-packages");
	input.format = args::format_arg(matches).to_string();
}

/// Read repeated job names, defaulting to a single `default` job.
fn collect_jobs(matches: &ArgMatches) -> Vec<String> {
	matches.get_many::<String>("job").map_or_else(
		|| vec!["default".to_string()],
		|values| values.cloned().collect(),
	)
}

/// Read explicit changed files, falling back to a git diff when refs are given.
fn collect_changed_files(root: &Path, matches: &ArgMatches) -> Vec<String> {
	let explicit = args::collect_normalized_paths(matches, "changed-files");
	let base_ref = matches.get_one::<String>("base").map(String::as_str);
	let head_ref = matches.get_one::<String>("head").map(String::as_str);

	if explicit.is_empty() && (base_ref.is_some() || head_ref.is_some()) {
		return detect_changed_files_from_git(root, base_ref, head_ref);
	}

	explicit
}
