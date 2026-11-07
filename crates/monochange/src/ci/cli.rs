//! The `monochange ci` clap command tree.
//!
//! Argument definitions are grouped by purpose and concatenated in a fixed
//! order. The ordering is deliberate: `--help` output is snapshot tested, so new
//! flags belong at the end of the group they conceptually belong to.

use clap::Arg;
use clap::ArgAction;
use clap::Command;
use clap::value_parser;

/// Accepted provider names for `ci decide`.
const PROVIDER_VALUES: [&str; 6] = [
	"opencode-go",
	"openrouter",
	"cloudflare",
	"jev",
	"openai-compatible",
	"graph",
];
/// Accepted aliases for `--prior-decision`.
const PRIOR_DECISION_VALUES: [&str; 6] = ["y", "n", "true", "false", "run", "skip"];
/// Accepted conclusions for `ci verify`.
const CONCLUSION_VALUES: [&str; 4] = ["success", "failure", "cancelled", "skipped"];

/// Build the `monochange ci` subcommand tree.
pub(crate) fn build_ci_subcommand() -> Command {
	Command::new("ci")
		.about(
			"Evaluate conditional CI gates, verify main branch outcomes, and view repo-scoped analytics",
		)
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(build_ci_auth_subcommand())
		.subcommand(build_ci_decide_subcommand())
		.subcommand(build_ci_verify_subcommand())
		.subcommand(build_ci_analytics_subcommand())
		.subcommand(build_ci_history_subcommand())
}

/// Build the `ci auth` subcommand tree.
fn build_ci_auth_subcommand() -> Command {
	Command::new("auth")
		.about("Manage repository-scoped authentication for CI history and analytics")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(build_auth_init_subcommand())
		.subcommand(build_auth_verify_subcommand())
		.subcommand(build_auth_token_subcommand())
}

/// Build `ci auth init`.
fn build_auth_init_subcommand() -> Command {
	Command::new("init")
		.about("Generate a cryptographic secret and issue a repo-scoped HMAC token")
		.arg(repo_id_arg())
		.arg(
			Arg::new("force")
				.long("force")
				.action(ArgAction::SetTrue)
				.help("Overwrite an existing .monochange/ci-auth.json file"),
		)
		.arg(text_json_format_arg())
}

/// Build `ci auth verify`.
fn build_auth_verify_subcommand() -> Command {
	Command::new("verify")
		.about("Verify that a token matches this repository's canonical identity and secret")
		.arg(
			Arg::new("token").long("token").value_name("TOKEN").help(
				"Token to verify (defaults to MONOCHANGE_CI_TOKEN or .monochange/ci-auth.json)",
			),
		)
		.arg(repo_id_arg())
		.arg(text_json_format_arg())
}

/// Build `ci auth token`.
fn build_auth_token_subcommand() -> Command {
	Command::new("token")
		.about("Print the active repo-scoped token for use in CI secrets")
		.arg(repo_id_arg())
		.arg(text_json_format_arg())
}

/// Build `ci decide`.
fn build_ci_decide_subcommand() -> Command {
	Command::new("decide")
		.about("Evaluate whether CI jobs should run (y/n) using provider or workspace graph")
		.args(decide_target_args())
		.args(decide_change_args())
		.args(decide_context_args())
		.args(decide_unconditional_args())
		.args(decide_limit_args())
		.args(decide_scope_args())
		.args(decide_auth_args())
		.arg(decide_format_arg())
}

/// Decision target and provider settings.
fn decide_target_args() -> Vec<Arg> {
	vec![
		Arg::new("job")
			.long("job")
			.value_name("JOB")
			.action(ArgAction::Append)
			.help("Job name to evaluate; repeat to evaluate multiple jobs"),
		Arg::new("provider")
			.long("provider")
			.value_name("PROVIDER")
			.default_value("opencode-go")
			.value_parser(PROVIDER_VALUES)
			.help("Decision provider to use"),
		Arg::new("model").long("model").value_name("MODEL").help(
			"Model identifier for AI decision providers (default: deepseek-v4.1 for opencode-go)",
		),
		Arg::new("api-url")
			.long("api-url")
			.value_name("URL")
			.help("Override decision provider endpoint URL"),
		Arg::new("api-key")
			.long("api-key")
			.value_name("KEY")
			.help("API key or token for the decision provider"),
		Arg::new("account-id")
			.long("account-id")
			.value_name("ACCOUNT_ID")
			.help("Cloudflare account ID for the cloudflare provider"),
	]
}

/// Changed-file detection settings.
fn decide_change_args() -> Vec<Arg> {
	vec![
		Arg::new("changed-files")
			.long("changed-files")
			.alias("changed-paths")
			.value_name("PATH")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Changed file paths relative to repository root"),
		Arg::new("base")
			.long("base")
			.value_name("REF")
			.help("Git base ref used to detect changed files when --changed-files is omitted"),
		Arg::new("head")
			.long("head")
			.value_name("REF")
			.help("Git head ref used to detect changed files when --changed-files is omitted"),
	]
}

/// CI event, branch, and commit context.
fn decide_context_args() -> Vec<Arg> {
	vec![
		Arg::new("event")
			.long("event")
			.value_name("EVENT")
			.help("CI event name (e.g. pull_request, push, schedule, workflow_dispatch)"),
		Arg::new("branch")
			.long("branch")
			.value_name("BRANCH")
			.help("Branch name (e.g. main, release/1.0)"),
		Arg::new("ref")
			.long("ref")
			.value_name("REF")
			.help("Full git ref (e.g. refs/heads/main, refs/tags/v1.0.0)"),
		Arg::new("labels")
			.long("labels")
			.alias("label")
			.value_name("LABEL")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Pull request labels"),
		Arg::new("commit-message")
			.long("commit-message")
			.value_name("MESSAGE")
			.help("Head commit message to inspect for unconditional markers like [ci:full]"),
		Arg::new("commit")
			.long("commit")
			.value_name("SHA")
			.help("Commit SHA associated with this decision"),
	]
}

/// Flags and patterns that force a full run.
fn decide_unconditional_args() -> Vec<Arg> {
	vec![
		Arg::new("unconditional")
			.long("unconditional")
			.action(ArgAction::SetTrue)
			.help("Force unconditional execution (`y`) for all evaluated jobs"),
		Arg::new("unconditional-events")
			.long("unconditional-events")
			.value_name("EVENT")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Events that force unconditional runs (defaults to schedule,workflow_dispatch)"),
		Arg::new("unconditional-branches")
			.long("unconditional-branches")
			.value_name("PATTERN")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Branch or ref globs that force unconditional runs"),
		Arg::new("unconditional-paths")
			.long("unconditional-paths")
			.value_name("PATTERN")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Changed file globs that force unconditional runs"),
	]
}

/// Skip counters and duration thresholds.
fn decide_limit_args() -> Vec<Arg> {
	vec![
		Arg::new("max-skips-on-main")
			.long("max-skips-on-main")
			.value_name("COUNT")
			.default_value("5")
			.value_parser(value_parser!(u32))
			.help(
				"Maximum consecutive skips on main before forcing an unconditional verification run",
			),
		Arg::new("duration-seconds")
			.long("duration-seconds")
			.value_name("SECONDS")
			.value_parser(value_parser!(u64))
			.help("Current or expected job duration in seconds for red-flag checking"),
		Arg::new("max-duration-minutes")
			.long("max-duration-minutes")
			.value_name("MINUTES")
			.default_value("10")
			.value_parser(value_parser!(u64))
			.help("Duration red-flag threshold in minutes for general CI jobs"),
		Arg::new("rust-max-duration-minutes")
			.long("rust-max-duration-minutes")
			.value_name("MINUTES")
			.default_value("20")
			.value_parser(value_parser!(u64))
			.help("Duration red-flag threshold in minutes for Rust CI jobs"),
	]
}

/// Explicit job scope settings.
fn decide_scope_args() -> Vec<Arg> {
	vec![
		Arg::new("job-paths")
			.long("job-paths")
			.value_name("PATTERN")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Explicit file path globs that this job covers"),
		Arg::new("job-packages")
			.long("job-packages")
			.value_name("PACKAGE")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Explicit package IDs or names that this job covers"),
	]
}

/// Auth and identity settings shared by protected subcommands.
fn decide_auth_args() -> Vec<Arg> {
	vec![
		Arg::new("require-auth")
			.long("require-auth")
			.action(ArgAction::SetTrue)
			.help("Require repository-scoped authentication before loading or updating CI history"),
		Arg::new("token")
			.long("token")
			.value_name("TOKEN")
			.help("Repository-scoped CI auth token"),
		repo_id_arg(),
	]
}

/// Build `ci verify`.
fn build_ci_verify_subcommand() -> Command {
	Command::new("verify")
		.about("Record actual CI job outcomes and update closed-loop learned rules")
		.args(verify_outcome_args())
		.args(verify_decision_args())
		.args(verify_duration_args())
		.args(verify_auth_args())
		.arg(text_json_format_arg())
}

/// Required outcome arguments for `ci verify`.
fn verify_outcome_args() -> Vec<Arg> {
	vec![
		Arg::new("job")
			.long("job")
			.required(true)
			.value_name("JOB")
			.help("Job name that was executed or verified"),
		Arg::new("conclusion")
			.long("conclusion")
			.required(true)
			.value_name("CONCLUSION")
			.value_parser(CONCLUSION_VALUES)
			.help("Actual job outcome"),
		Arg::new("duration-seconds")
			.long("duration-seconds")
			.value_name("SECONDS")
			.default_value("0")
			.value_parser(value_parser!(u64))
			.help("Actual job execution duration in seconds"),
		Arg::new("commit")
			.long("commit")
			.value_name("SHA")
			.help("Commit SHA verified"),
		Arg::new("pr")
			.long("pr")
			.value_name("PR")
			.help("Pull request number or identifier"),
		Arg::new("branch")
			.long("branch")
			.value_name("BRANCH")
			.help("Branch where verification ran (defaults to main)"),
	]
}

/// Prior decision and affected files for `ci verify`.
fn verify_decision_args() -> Vec<Arg> {
	vec![
		Arg::new("prior-decision")
			.long("prior-decision")
			.value_name("DECISION")
			.value_parser(PRIOR_DECISION_VALUES)
			.help("Explicit prior gate decision if not already recorded in ci-memory.json"),
		Arg::new("redundant")
			.long("redundant")
			.action(ArgAction::SetTrue)
			.help("Mark a successful run (`y`) as redundant (`false_positive`)"),
		Arg::new("changed-files")
			.long("changed-files")
			.alias("changed-paths")
			.value_name("PATH")
			.action(ArgAction::Append)
			.value_delimiter(',')
			.help("Changed file paths associated with this verification run"),
	]
}

/// Duration thresholds for `ci verify`.
fn verify_duration_args() -> Vec<Arg> {
	vec![
		Arg::new("max-duration-minutes")
			.long("max-duration-minutes")
			.value_name("MINUTES")
			.default_value("10")
			.value_parser(value_parser!(u64))
			.help("Duration red-flag threshold in minutes for general CI jobs"),
		Arg::new("rust-max-duration-minutes")
			.long("rust-max-duration-minutes")
			.value_name("MINUTES")
			.default_value("20")
			.value_parser(value_parser!(u64))
			.help("Duration red-flag threshold in minutes for Rust CI jobs"),
	]
}

/// Auth settings for `ci verify`.
fn verify_auth_args() -> Vec<Arg> {
	vec![
		Arg::new("require-auth")
			.long("require-auth")
			.action(ArgAction::SetTrue)
			.help("Require repository-scoped authentication before loading or updating CI history"),
		Arg::new("token")
			.long("token")
			.value_name("TOKEN")
			.help("Repository-scoped CI auth token"),
		repo_id_arg(),
	]
}

/// Build `ci analytics`.
fn build_ci_analytics_subcommand() -> Command {
	Command::new("analytics")
		.about(
			"Compute protected CI decision accuracy, false negatives/positives, and duration analytics",
		)
		.args(report_auth_args())
		.args(report_duration_args())
		.arg(markdown_format_arg())
}

/// Build `ci history`.
fn build_ci_history_subcommand() -> Command {
	Command::new("history")
		.about("View protected repository CI gate decision and verification history")
		.arg(
			Arg::new("job")
				.long("job")
				.value_name("JOB")
				.help("Filter history to a specific job"),
		)
		.arg(
			Arg::new("limit")
				.long("limit")
				.value_name("COUNT")
				.value_parser(value_parser!(usize))
				.help("Maximum number of entries to return"),
		)
		.args(report_auth_args())
		.arg(markdown_format_arg())
}

/// Auth settings shared by analytics and history.
fn report_auth_args() -> Vec<Arg> {
	vec![
		Arg::new("token")
			.long("token")
			.value_name("TOKEN")
			.help("Repository-scoped CI auth token"),
		repo_id_arg(),
	]
}

/// Duration thresholds shared by analytics and verify.
fn report_duration_args() -> Vec<Arg> {
	vec![
		Arg::new("max-duration-minutes")
			.long("max-duration-minutes")
			.value_name("MINUTES")
			.default_value("10")
			.value_parser(value_parser!(u64))
			.help("Duration red-flag threshold in minutes for general CI jobs"),
		Arg::new("rust-max-duration-minutes")
			.long("rust-max-duration-minutes")
			.value_name("MINUTES")
			.default_value("20")
			.value_parser(value_parser!(u64))
			.help("Duration red-flag threshold in minutes for Rust CI jobs"),
	]
}

/// Shared `--repo-id` override flag.
fn repo_id_arg() -> Arg {
	Arg::new("repo-id")
		.long("repo-id")
		.value_name("REPO_ID")
		.help("Override canonical repository identity")
}

/// `--format` flag limited to text and JSON.
fn text_json_format_arg() -> Arg {
	Arg::new("format")
		.long("format")
		.value_name("FORMAT")
		.default_value("text")
		.value_parser(["text", "json"])
}

/// `--format` flag that also supports GitHub Actions outputs.
fn decide_format_arg() -> Arg {
	Arg::new("format")
		.long("format")
		.value_name("FORMAT")
		.default_value("text")
		.value_parser(["text", "json", "github-outputs"])
}

/// `--format` flag that also supports Markdown.
fn markdown_format_arg() -> Arg {
	Arg::new("format")
		.long("format")
		.value_name("FORMAT")
		.default_value("text")
		.value_parser(["text", "markdown", "md", "json"])
}
