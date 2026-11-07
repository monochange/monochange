//! CLI and environment parsing for `ci decide`.
//!
//! The parsed input is a flat snapshot of everything the decision pipeline
//! needs, so the evaluator never reaches back into `clap` matches or the
//! environment while it is running. Job scope, provider, and unconditional
//! patterns come from `[ci]` in `monochange.toml`; CLI flags only override
//! them for one run.

use std::path::Path;
use std::path::PathBuf;

use clap::ArgMatches;
use monochange_config::CiSettings;
use monochange_config::load_ci_settings;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use super::super::DEFAULT_MAX_DURATION_MINUTES;
use super::super::DEFAULT_RUST_MAX_DURATION_MINUTES;
use super::super::args;
use super::super::graph::detect_changed_files_from_git;
use super::super::types::CiDecisionProvider;

/// Everything `ci decide` needs once CLI arguments and environment are resolved.
pub(super) struct DecideInput {
	/// Job ids to evaluate, from `--job` or `[ci.jobs]` keys.
	pub(super) jobs: Vec<String>,
	/// Resolved `[ci]` settings from `monochange.toml`.
	pub(super) settings: CiSettings,
	/// Provider selected by `--provider` or `[ci]` provider.
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
	pub(super) explicit_duration: Option<u64>,
	pub(super) max_duration_minutes: u64,
	pub(super) rust_max_duration_minutes: u64,
	pub(super) format: String,
	/// `$GITHUB_OUTPUT` path read once at the CLI boundary, when configured.
	pub(super) github_output: Option<PathBuf>,
}

/// Parse CLI arguments, environment, and `[ci]` configuration into input.
///
/// `github_output` comes from the caller so the whole run reads
/// `$GITHUB_OUTPUT` exactly once, including the `--on-error run-all` path.
pub(super) fn parse_decide_input(
	root: &Path,
	matches: &ArgMatches,
	github_output: Option<&Path>,
) -> MonochangeResult<DecideInput> {
	let settings = load_ci_settings(root)?;
	let provider = resolve_provider(matches, &settings)?;
	let jobs = collect_jobs(matches, &settings)?;
	let mut input = DecideInput {
		jobs,
		settings,
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
		explicit_duration: None,
		max_duration_minutes: DEFAULT_MAX_DURATION_MINUTES,
		rust_max_duration_minutes: DEFAULT_RUST_MAX_DURATION_MINUTES,
		format: "text".to_string(),
		github_output: github_output.map(PathBuf::from),
	};

	input.changed_files = collect_changed_files(root, matches)?;

	fill_provider_settings(&mut input, matches);
	fill_invocation_context(&mut input, matches);
	fill_limits(&mut input, matches);
	input.format = args::format_arg(matches).to_string();

	Ok(input)
}

/// Resolve the provider from `--provider` or the `[ci]` configuration.
fn resolve_provider(
	matches: &ArgMatches,
	settings: &CiSettings,
) -> MonochangeResult<CiDecisionProvider> {
	let provider_name = matches
		.get_one::<String>("provider")
		.map_or(settings.provider.as_str(), String::as_str);

	CiDecisionProvider::parse(provider_name)
}

/// Read repeated job names, defaulting to every configured `[ci.jobs]` key.
fn collect_jobs(matches: &ArgMatches, settings: &CiSettings) -> MonochangeResult<Vec<String>> {
	if let Some(values) = matches.get_many::<String>("job") {
		let jobs: Vec<String> = values.cloned().collect();

		return Ok(jobs);
	}

	if settings.jobs.is_empty() {
		return Err(MonochangeError::Config(
			"no `--job` given and no `[ci.jobs]` configured in monochange.toml; declare the job or pass `--job`"
				.to_string(),
		));
	}

	Ok(settings.jobs.keys().cloned().collect())
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
	input.unconditional_flag = matches.get_flag("unconditional");
}

/// Skip counters and duration thresholds.
fn fill_limits(input: &mut DecideInput, matches: &ArgMatches) {
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

/// Read explicit changed files, falling back to a git diff when refs are given.
///
/// A failing git diff is an error, not an empty change set: empty would skip
/// every gated job.
fn collect_changed_files(root: &Path, matches: &ArgMatches) -> MonochangeResult<Vec<String>> {
	let explicit = args::collect_normalized_paths(matches, "changed-files");
	let base_ref = matches.get_one::<String>("base").map(String::as_str);
	let head_ref = matches.get_one::<String>("head").map(String::as_str);

	if explicit.is_empty() && (base_ref.is_some() || head_ref.is_some()) {
		return detect_changed_files_from_git(root, base_ref, head_ref);
	}

	Ok(explicit)
}
