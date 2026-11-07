use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use chrono::Utc;
use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use clap::Command;
use glob::Pattern;
use monochange_config::load_workspace_configuration;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePathMatch;
use monochange_core::PackagePathMatcher;
use monochange_core::materialize_dependency_edges;
use reqwest::Client;
use ring::digest;
use ring::hmac;
use ring::rand::SecureRandom;
use ring::rand::SystemRandom;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use crate::changeset_policy::normalize_changed_path;
use crate::discover_workspace;

pub const CI_AUTH_RELATIVE_PATH: &str = ".monochange/ci-auth.json";
pub const CI_MEMORY_RELATIVE_PATH: &str = ".monochange/ci-memory.json";
pub const DEFAULT_OPENCODE_GO_API_URL: &str = "https://api.opencode.ai/v1/chat/completions";
pub const DEFAULT_OPENCODE_GO_MODEL: &str = "deepseek-v4.1";
pub const DEFAULT_OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const DEFAULT_OPENROUTER_MODEL: &str = "deepseek/deepseek-chat";
pub const DEFAULT_CLOUDFLARE_MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
pub const DEFAULT_MAX_SKIPS_ON_MAIN: u32 = 5;
pub const DEFAULT_MAX_DURATION_MINUTES: u64 = 10;
pub const DEFAULT_RUST_MAX_DURATION_MINUTES: u64 = 20;

const DEFAULT_UNCONDITIONAL_EVENTS: &[&str] = &["schedule", "workflow_dispatch"];
const DEFAULT_UNCONDITIONAL_BRANCHES: &[&str] = &[
	"release/*",
	"monochange/release/*",
	"refs/tags/v*",
	"refs/heads/release/*",
	"refs/heads/monochange/release/*",
];
const DEFAULT_UNCONDITIONAL_PATHS: &[&str] = &[
	"Cargo.lock",
	"rust-toolchain.toml",
	"rust-toolchain",
	".github/workflows/*",
	".github/workflows/**",
];
const UNCONDITIONAL_MARKERS: &[&str] = &[
	"[ci:full]",
	"[ci:unconditional]",
	"ci:full",
	"ci:unconditional",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiAuthFile {
	pub schema_version: u32,
	pub repo_id: String,
	pub repo_hash: String,
	pub secret: String,
	pub token: String,
	pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiAuthReport {
	pub action: String,
	pub authenticated: bool,
	pub repo_id: String,
	pub repo_hash: String,
	pub token: String,
	pub auth_file: String,
	pub source: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CiDecisionProvider {
	OpencodeGo,
	Openrouter,
	Cloudflare,
	Jev,
	OpenaiCompatible,
	Graph,
}

impl CiDecisionProvider {
	pub fn parse(value: &str) -> MonochangeResult<Self> {
		match value.trim() {
			"opencode-go" => Ok(Self::OpencodeGo),
			"openrouter" => Ok(Self::Openrouter),
			"cloudflare" => Ok(Self::Cloudflare),
			"jev" => Ok(Self::Jev),
			"openai-compatible" => Ok(Self::OpenaiCompatible),
			"graph" => Ok(Self::Graph),
			other => {
				Err(MonochangeError::Config(format!(
					"unsupported CI decision provider `{other}`; expected one of: opencode-go, openrouter, cloudflare, jev, openai-compatible, graph"
				)))
			}
		}
	}

	pub fn as_str(self) -> &'static str {
		match self {
			Self::OpencodeGo => "opencode-go",
			Self::Openrouter => "openrouter",
			Self::Cloudflare => "cloudflare",
			Self::Jev => "jev",
			Self::OpenaiCompatible => "openai-compatible",
			Self::Graph => "graph",
		}
	}
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiVerificationClassification {
	TruePositive,
	TrueNegative,
	FalsePositive,
	FalseNegative,
}

impl CiVerificationClassification {
	pub fn as_str(self) -> &'static str {
		match self {
			Self::TruePositive => "true_positive",
			Self::TrueNegative => "true_negative",
			Self::FalsePositive => "false_positive",
			Self::FalseNegative => "false_negative",
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiLearnedRule {
	pub id: String,
	pub job: String,
	pub patterns: Vec<String>,
	#[serde(default)]
	pub packages: Vec<String>,
	pub reason: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_commit: Option<String>,
	pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiDecisionRecord {
	pub id: String,
	pub repo_id: String,
	pub job: String,
	pub decision: String,
	pub run: bool,
	pub unconditional: bool,
	pub provider: String,
	pub model: String,
	pub reason: String,
	#[serde(default)]
	pub unconditional_reasons: Vec<String>,
	#[serde(default)]
	pub matched_learned_rules: Vec<String>,
	#[serde(default)]
	pub affected_packages: Vec<String>,
	#[serde(default)]
	pub changed_files: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub event: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub branch: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub commit: Option<String>,
	pub consecutive_skips_on_main: u32,
	pub duration_red_flag: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub observed_duration_seconds: Option<u64>,
	pub threshold_minutes: u64,
	#[serde(default)]
	pub speed_suggestions: Vec<String>,
	pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiVerificationRecord {
	pub id: String,
	pub repo_id: String,
	pub job: String,
	pub conclusion: String,
	pub duration_seconds: u64,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub commit: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub pr: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub branch: Option<String>,
	pub prior_decision: String,
	pub classification: CiVerificationClassification,
	pub reason: String,
	#[serde(default)]
	pub changed_files: Vec<String>,
	#[serde(default)]
	pub affected_packages: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub learned_rule_added: Option<CiLearnedRule>,
	pub duration_red_flag: bool,
	pub threshold_minutes: u64,
	#[serde(default)]
	pub speed_suggestions: Vec<String>,
	pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiMemoryStore {
	pub schema_version: u32,
	pub repo_id: String,
	pub repo_hash: String,
	#[serde(default)]
	pub consecutive_skips_on_main: BTreeMap<String, u32>,
	#[serde(default)]
	pub learned_rules: Vec<CiLearnedRule>,
	#[serde(default)]
	pub decisions: Vec<CiDecisionRecord>,
	#[serde(default)]
	pub verifications: Vec<CiVerificationRecord>,
}

impl CiMemoryStore {
	pub fn new(repo_id: String) -> Self {
		let repo_hash = compute_repo_hash(&repo_id);
		Self {
			schema_version: 1,
			repo_id,
			repo_hash,
			consecutive_skips_on_main: BTreeMap::new(),
			learned_rules: Vec::new(),
			decisions: Vec::new(),
			verifications: Vec::new(),
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiDecideReport {
	pub repo_id: String,
	pub decision: String,
	pub run: bool,
	pub unconditional: bool,
	pub provider: String,
	pub model: String,
	pub reason: String,
	pub jobs: Vec<CiDecisionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiSlowJobReport {
	pub job: String,
	pub max_duration_seconds: u64,
	pub average_duration_seconds: f64,
	pub threshold_minutes: u64,
	pub is_rust_job: bool,
	pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiFalseNegativeIncident {
	pub id: String,
	pub job: String,
	pub commit: Option<String>,
	pub pr: Option<String>,
	pub changed_files: Vec<String>,
	pub reason: String,
	pub learned_rule: Option<CiLearnedRule>,
	pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiFalsePositiveIncident {
	pub id: String,
	pub job: String,
	pub commit: Option<String>,
	pub pr: Option<String>,
	pub changed_files: Vec<String>,
	pub wasted_vm_minutes: f64,
	pub reason: String,
	pub recommendation: String,
	pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiAnalyticsReport {
	pub repo_id: String,
	pub repo_hash: String,
	pub authenticated: bool,
	pub total_decisions: usize,
	pub run_decisions: usize,
	pub skip_decisions: usize,
	pub unconditional_decisions: usize,
	pub skip_rate: f64,
	pub estimated_vm_minutes_saved: f64,
	pub wasted_vm_minutes: f64,
	pub true_positives: usize,
	pub true_negatives: usize,
	pub false_positives: usize,
	pub false_negatives: usize,
	pub precision: f64,
	pub recall: f64,
	pub false_negative_rate: f64,
	pub learned_rules_count: usize,
	pub false_negative_incidents: Vec<CiFalseNegativeIncident>,
	pub false_positive_incidents: Vec<CiFalsePositiveIncident>,
	pub tuning_recommendations: Vec<String>,
	pub slow_jobs: Vec<CiSlowJobReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiHistoryReport {
	pub repo_id: String,
	pub repo_hash: String,
	pub authenticated: bool,
	pub learned_rules: Vec<CiLearnedRule>,
	pub decisions: Vec<CiDecisionRecord>,
	pub verifications: Vec<CiVerificationRecord>,
}

/// Build the `monochange ci` CLI subcommand tree.
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

fn build_ci_auth_subcommand() -> Command {
	Command::new("auth")
		.about("Manage repository-scoped authentication for CI history and analytics")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("init")
				.about("Generate a cryptographic secret and issue a repo-scoped HMAC token")
				.arg(
					Arg::new("repo-id")
						.long("repo-id")
						.value_name("REPO_ID")
						.help("Override canonical repository identity"),
				)
				.arg(
					Arg::new("force")
						.long("force")
						.action(ArgAction::SetTrue)
						.help("Overwrite an existing .monochange/ci-auth.json file"),
				)
				.arg(
					Arg::new("format")
						.long("format")
						.value_name("FORMAT")
						.default_value("text")
						.value_parser(["text", "json"]),
				),
		)
		.subcommand(
			Command::new("verify")
				.about(
					"Verify that a token matches this repository's canonical identity and secret",
				)
				.arg(Arg::new("token").long("token").value_name("TOKEN").help(
					"Token to verify (defaults to MONOCHANGE_CI_TOKEN or .monochange/ci-auth.json)",
				))
				.arg(
					Arg::new("repo-id")
						.long("repo-id")
						.value_name("REPO_ID")
						.help("Override canonical repository identity"),
				)
				.arg(
					Arg::new("format")
						.long("format")
						.value_name("FORMAT")
						.default_value("text")
						.value_parser(["text", "json"]),
				),
		)
		.subcommand(
			Command::new("token")
				.about("Print the active repo-scoped token for use in CI secrets")
				.arg(
					Arg::new("repo-id")
						.long("repo-id")
						.value_name("REPO_ID")
						.help("Override canonical repository identity"),
				)
				.arg(
					Arg::new("format")
						.long("format")
						.value_name("FORMAT")
						.default_value("text")
						.value_parser(["text", "json"]),
				),
		)
}

fn build_ci_decide_subcommand() -> Command {
	Command::new("decide")
		.about("Evaluate whether CI jobs should run (y/n) using provider or workspace graph")
		.arg(
			Arg::new("job")
				.long("job")
				.value_name("JOB")
				.action(ArgAction::Append)
				.help("Job name to evaluate; repeat to evaluate multiple jobs"),
		)
		.arg(
			Arg::new("provider")
				.long("provider")
				.value_name("PROVIDER")
				.default_value("opencode-go")
				.value_parser([
					"opencode-go",
					"openrouter",
					"cloudflare",
					"jev",
					"openai-compatible",
					"graph",
				])
				.help("Decision provider to use"),
		)
		.arg(Arg::new("model").long("model").value_name("MODEL").help(
			"Model identifier for AI decision providers (default: deepseek-v4.1 for opencode-go)",
		))
		.arg(
			Arg::new("api-url")
				.long("api-url")
				.value_name("URL")
				.help("Override decision provider endpoint URL"),
		)
		.arg(
			Arg::new("api-key")
				.long("api-key")
				.value_name("KEY")
				.help("API key or token for the decision provider"),
		)
		.arg(
			Arg::new("account-id")
				.long("account-id")
				.value_name("ACCOUNT_ID")
				.help("Cloudflare account ID for the cloudflare provider"),
		)
		.arg(
			Arg::new("changed-files")
				.long("changed-files")
				.alias("changed-paths")
				.value_name("PATH")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Changed file paths relative to repository root"),
		)
		.arg(
			Arg::new("base")
				.long("base")
				.value_name("REF")
				.help("Git base ref used to detect changed files when --changed-files is omitted"),
		)
		.arg(
			Arg::new("head")
				.long("head")
				.value_name("REF")
				.help("Git head ref used to detect changed files when --changed-files is omitted"),
		)
		.arg(
			Arg::new("event")
				.long("event")
				.value_name("EVENT")
				.help("CI event name (e.g. pull_request, push, schedule, workflow_dispatch)"),
		)
		.arg(
			Arg::new("branch")
				.long("branch")
				.value_name("BRANCH")
				.help("Branch name (e.g. main, release/1.0)"),
		)
		.arg(
			Arg::new("ref")
				.long("ref")
				.value_name("REF")
				.help("Full git ref (e.g. refs/heads/main, refs/tags/v1.0.0)"),
		)
		.arg(
			Arg::new("labels")
				.long("labels")
				.alias("label")
				.value_name("LABEL")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Pull request labels"),
		)
		.arg(
			Arg::new("commit-message")
				.long("commit-message")
				.value_name("MESSAGE")
				.help("Head commit message to inspect for unconditional markers like [ci:full]"),
		)
		.arg(
			Arg::new("commit")
				.long("commit")
				.value_name("SHA")
				.help("Commit SHA associated with this decision"),
		)
		.arg(
			Arg::new("unconditional")
				.long("unconditional")
				.action(ArgAction::SetTrue)
				.help("Force unconditional execution (`y`) for all evaluated jobs"),
		)
		.arg(
			Arg::new("unconditional-events")
				.long("unconditional-events")
				.value_name("EVENT")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help(
					"Events that force unconditional runs (defaults to schedule,workflow_dispatch)",
				),
		)
		.arg(
			Arg::new("unconditional-branches")
				.long("unconditional-branches")
				.value_name("PATTERN")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Branch or ref globs that force unconditional runs"),
		)
		.arg(
			Arg::new("unconditional-paths")
				.long("unconditional-paths")
				.value_name("PATTERN")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Changed file globs that force unconditional runs"),
		)
		.arg(
			Arg::new("max-skips-on-main")
				.long("max-skips-on-main")
				.value_name("COUNT")
				.default_value("5")
				.value_parser(clap::value_parser!(u32))
				.help(
					"Maximum consecutive skips on main before forcing an unconditional verification run",
				),
		)
		.arg(
			Arg::new("duration-seconds")
				.long("duration-seconds")
				.value_name("SECONDS")
				.value_parser(clap::value_parser!(u64))
				.help("Current or expected job duration in seconds for red-flag checking"),
		)
		.arg(
			Arg::new("max-duration-minutes")
				.long("max-duration-minutes")
				.value_name("MINUTES")
				.default_value("10")
				.value_parser(clap::value_parser!(u64))
				.help("Duration red-flag threshold in minutes for general CI jobs"),
		)
		.arg(
			Arg::new("rust-max-duration-minutes")
				.long("rust-max-duration-minutes")
				.value_name("MINUTES")
				.default_value("20")
				.value_parser(clap::value_parser!(u64))
				.help("Duration red-flag threshold in minutes for Rust CI jobs"),
		)
		.arg(
			Arg::new("job-paths")
				.long("job-paths")
				.value_name("PATTERN")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Explicit file path globs that this job covers"),
		)
		.arg(
			Arg::new("job-packages")
				.long("job-packages")
				.value_name("PACKAGE")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Explicit package IDs or names that this job covers"),
		)
		.arg(
			Arg::new("require-auth")
				.long("require-auth")
				.action(ArgAction::SetTrue)
				.help(
					"Require repository-scoped authentication before loading or updating CI history",
				),
		)
		.arg(
			Arg::new("token")
				.long("token")
				.value_name("TOKEN")
				.help("Repository-scoped CI auth token"),
		)
		.arg(
			Arg::new("repo-id")
				.long("repo-id")
				.value_name("REPO_ID")
				.help("Override canonical repository identity"),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.value_name("FORMAT")
				.default_value("text")
				.value_parser(["text", "json", "github-outputs"])
				.help("Output format for decision results"),
		)
}

fn build_ci_verify_subcommand() -> Command {
	Command::new("verify")
		.about("Record actual CI job outcomes and update closed-loop learned rules")
		.arg(
			Arg::new("job")
				.long("job")
				.required(true)
				.value_name("JOB")
				.help("Job name that was executed or verified"),
		)
		.arg(
			Arg::new("conclusion")
				.long("conclusion")
				.required(true)
				.value_name("CONCLUSION")
				.value_parser(["success", "failure", "cancelled", "skipped"])
				.help("Actual job outcome"),
		)
		.arg(
			Arg::new("duration-seconds")
				.long("duration-seconds")
				.value_name("SECONDS")
				.default_value("0")
				.value_parser(clap::value_parser!(u64))
				.help("Actual job execution duration in seconds"),
		)
		.arg(
			Arg::new("commit")
				.long("commit")
				.value_name("SHA")
				.help("Commit SHA verified"),
		)
		.arg(
			Arg::new("pr")
				.long("pr")
				.value_name("PR")
				.help("Pull request number or identifier"),
		)
		.arg(
			Arg::new("branch")
				.long("branch")
				.value_name("BRANCH")
				.help("Branch where verification ran (defaults to main)"),
		)
		.arg(
			Arg::new("prior-decision")
				.long("prior-decision")
				.value_name("DECISION")
				.value_parser(["y", "n", "true", "false", "run", "skip"])
				.help("Explicit prior gate decision if not already recorded in ci-memory.json"),
		)
		.arg(
			Arg::new("redundant")
				.long("redundant")
				.action(ArgAction::SetTrue)
				.help("Mark a successful run (`y`) as redundant (`false_positive`)"),
		)
		.arg(
			Arg::new("changed-files")
				.long("changed-files")
				.alias("changed-paths")
				.value_name("PATH")
				.action(ArgAction::Append)
				.value_delimiter(',')
				.help("Changed file paths associated with this verification run"),
		)
		.arg(
			Arg::new("max-duration-minutes")
				.long("max-duration-minutes")
				.value_name("MINUTES")
				.default_value("10")
				.value_parser(clap::value_parser!(u64))
				.help("Duration red-flag threshold in minutes for general CI jobs"),
		)
		.arg(
			Arg::new("rust-max-duration-minutes")
				.long("rust-max-duration-minutes")
				.value_name("MINUTES")
				.default_value("20")
				.value_parser(clap::value_parser!(u64))
				.help("Duration red-flag threshold in minutes for Rust CI jobs"),
		)
		.arg(
			Arg::new("require-auth")
				.long("require-auth")
				.action(ArgAction::SetTrue)
				.help(
					"Require repository-scoped authentication before loading or updating CI history",
				),
		)
		.arg(
			Arg::new("token")
				.long("token")
				.value_name("TOKEN")
				.help("Repository-scoped CI auth token"),
		)
		.arg(
			Arg::new("repo-id")
				.long("repo-id")
				.value_name("REPO_ID")
				.help("Override canonical repository identity"),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.value_name("FORMAT")
				.default_value("text")
				.value_parser(["text", "json"])
				.help("Output format for verification result"),
		)
}

fn build_ci_analytics_subcommand() -> Command {
	Command::new("analytics")
		.about(
			"Compute protected CI decision accuracy, false negatives/positives, and duration analytics",
		)
		.arg(
			Arg::new("token")
				.long("token")
				.value_name("TOKEN")
				.help("Repository-scoped CI auth token"),
		)
		.arg(
			Arg::new("repo-id")
				.long("repo-id")
				.value_name("REPO_ID")
				.help("Override canonical repository identity"),
		)
		.arg(
			Arg::new("max-duration-minutes")
				.long("max-duration-minutes")
				.value_name("MINUTES")
				.default_value("10")
				.value_parser(clap::value_parser!(u64))
				.help("Duration red-flag threshold in minutes for general CI jobs"),
		)
		.arg(
			Arg::new("rust-max-duration-minutes")
				.long("rust-max-duration-minutes")
				.value_name("MINUTES")
				.default_value("20")
				.value_parser(clap::value_parser!(u64))
				.help("Duration red-flag threshold in minutes for Rust CI jobs"),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.value_name("FORMAT")
				.default_value("text")
				.value_parser(["text", "markdown", "md", "json"])
				.help("Output format for analytics report"),
		)
}

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
				.value_parser(clap::value_parser!(usize))
				.help("Maximum number of entries to return"),
		)
		.arg(
			Arg::new("token")
				.long("token")
				.value_name("TOKEN")
				.help("Repository-scoped CI auth token"),
		)
		.arg(
			Arg::new("repo-id")
				.long("repo-id")
				.value_name("REPO_ID")
				.help("Override canonical repository identity"),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.value_name("FORMAT")
				.default_value("text")
				.value_parser(["text", "markdown", "md", "json"])
				.help("Output format for history report"),
		)
}

/// Dispatch `monochange ci` subcommands.
pub(crate) async fn run_ci_subcommand(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	match matches.subcommand() {
		Some(("auth", auth_matches)) => run_ci_auth_subcommand(root, auth_matches),
		Some(("decide", decide_matches)) => run_ci_decide_subcommand(root, decide_matches).await,
		Some(("verify", verify_matches)) => run_ci_verify_subcommand(root, verify_matches),
		Some(("analytics", analytics_matches)) => {
			run_ci_analytics_subcommand(root, analytics_matches)
		}
		Some(("history", history_matches)) => run_ci_history_subcommand(root, history_matches),
		_ => {
			Err(MonochangeError::Config(
				"missing `monochange ci` subcommand".to_string(),
			))
		}
	}
}

/// Normalize a git remote URL or identifier into a canonical repository identity.
pub fn normalize_repo_identity(raw: &str) -> String {
	let trimmed = raw.trim().trim_end_matches('/');
	if let Some(rest) = trimmed.strip_prefix("git@")
		&& let Some((host, path)) = rest.split_once(':')
	{
		let clean_path = path
			.trim_start_matches('/')
			.strip_suffix(".git")
			.unwrap_or(path);
		return format!(
			"{}/{}",
			host.to_ascii_lowercase(),
			clean_path.to_ascii_lowercase()
		);
	}

	for scheme in ["https://", "http://", "ssh://", "git://"] {
		if let Some(rest) = trimmed.strip_prefix(scheme) {
			let without_user = rest.split_once('@').map_or(rest, |(_, after_at)| after_at);
			if let Some((host, path)) = without_user.split_once('/') {
				let clean_path = path
					.trim_start_matches('/')
					.strip_suffix(".git")
					.unwrap_or(path);
				return format!(
					"{}/{}",
					host.to_ascii_lowercase(),
					clean_path.to_ascii_lowercase()
				);
			}
		}
	}

	let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
	without_git.to_ascii_lowercase()
}

/// Derive the canonical repository identity (`repo_id`) for `root`.
///
/// Order of resolution:
/// 1. Explicit `--repo-id` override (or `MONOCHANGE_CI_REPO_ID` env var).
/// 2. Normalized `origin` remote URL from `.git/config` or `git config --get remote.origin.url`.
/// 3. Fallback to `local:<sha256_prefix>` of the canonical repository root path.
pub fn derive_canonical_repo_id(root: &Path, repo_id_override: Option<&str>) -> String {
	if let Some(override_id) = repo_id_override
		.map(str::trim)
		.filter(|value| !value.is_empty())
	{
		return normalize_repo_identity(override_id);
	}

	if let Ok(env_id) = std::env::var("MONOCHANGE_CI_REPO_ID") {
		let trimmed = env_id.trim();
		if !trimmed.is_empty() {
			return normalize_repo_identity(trimmed);
		}
	}

	if let Some(origin_url) = read_origin_url_from_git_config(root).or_else(|| {
		ProcessCommand::new("git")
			.current_dir(root)
			.args(["config", "--get", "remote.origin.url"])
			.output()
			.ok()
			.filter(|output| output.status.success())
			.and_then(|output| String::from_utf8(output.stdout).ok())
			.map(|value| value.trim().to_string())
			.filter(|value| !value.is_empty())
	}) {
		return normalize_repo_identity(&origin_url);
	}

	let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
	let digest_bytes = digest::digest(&digest::SHA256, canonical.to_string_lossy().as_bytes());
	let hash_hex = hex_encode(digest_bytes.as_ref());
	let prefix = hash_hex.get(..16).unwrap_or(&hash_hex);
	format!("local:{prefix}")
}

fn read_origin_url_from_git_config(root: &Path) -> Option<String> {
	let dot_git = root.join(".git");
	let config_path = if dot_git.is_dir() {
		dot_git.join("config")
	} else if dot_git.is_file() {
		let content = fs::read_to_string(&dot_git).ok()?;
		let gitdir_raw = content
			.lines()
			.find_map(|line| line.strip_prefix("gitdir:"))?
			.trim();
		let gitdir_path = Path::new(gitdir_raw);
		let resolved = if gitdir_path.is_absolute() {
			gitdir_path.to_path_buf()
		} else {
			root.join(gitdir_path)
		};
		if resolved.join("config").is_file() {
			resolved.join("config")
		} else if resolved
			.parent()
			.and_then(Path::parent)
			.is_some_and(|common| common.join("config").is_file())
		{
			resolved.parent()?.parent()?.join("config")
		} else {
			return None;
		}
	} else {
		return None;
	};

	let config_text = fs::read_to_string(config_path).ok()?;
	let mut in_origin = false;
	for line in config_text.lines() {
		let trimmed = line.trim();
		if trimmed.starts_with('[') && trimmed.ends_with(']') {
			in_origin = trimmed.eq_ignore_ascii_case("[remote \"origin\"]");
			continue;
		}
		if in_origin
			&& let Some((key, value)) = trimmed.split_once('=')
			&& key.trim().eq_ignore_ascii_case("url")
		{
			let url = value.trim().trim_matches('"');
			if !url.is_empty() {
				return Some(url.to_string());
			}
		}
	}
	None
}

/// Compute the 16-char hex hash of the canonical `repo_id`.
pub fn compute_repo_hash(repo_id: &str) -> String {
	let normalized = normalize_repo_identity(repo_id);
	let digest_bytes = digest::digest(&digest::SHA256, normalized.as_bytes());
	let full = hex_encode(digest_bytes.as_ref());
	full.get(..16).unwrap_or(&full).to_string()
}

/// Issue a repo-scoped HMAC-SHA256 token `mc_ci_<repo_hash>_<hmac>`.
pub fn issue_repo_token(repo_id: &str, secret: &str) -> String {
	let normalized = normalize_repo_identity(repo_id);
	let repo_hash = compute_repo_hash(&normalized);
	let key = hmac::Key::new(hmac::HMAC_SHA256, secret.trim().as_bytes());
	let payload = format!("monochange-ci-auth:v1:{normalized}:{repo_hash}");
	let tag = hmac::sign(&key, payload.as_bytes());
	let hmac_hex = hex_encode(tag.as_ref());
	format!("mc_ci_{repo_hash}_{hmac_hex}")
}

/// Verify that `token` is structurally valid and matches `expected_repo_id` and `secret`.
pub fn verify_repo_token(
	token: &str,
	expected_repo_id: &str,
	secret: &str,
) -> MonochangeResult<()> {
	let trimmed = token.trim();
	let Some(rest) = trimmed.strip_prefix("mc_ci_") else {
		return Err(MonochangeError::Config(
			"invalid CI auth token format: expected token starting with `mc_ci_`".to_string(),
		));
	};
	let Some((token_repo_hash, token_hmac_hex)) = rest.split_once('_') else {
		return Err(MonochangeError::Config(
			"invalid CI auth token format: expected `mc_ci_<repo_hash>_<hmac>`".to_string(),
		));
	};

	let normalized_repo_id = normalize_repo_identity(expected_repo_id);
	let expected_repo_hash = compute_repo_hash(&normalized_repo_id);
	if token_repo_hash != expected_repo_hash {
		return Err(MonochangeError::Config(format!(
			"CI auth token repository mismatch: token belongs to repo hash `{token_repo_hash}`, but current repository `{normalized_repo_id}` has hash `{expected_repo_hash}` (cross-repo access denied)"
		)));
	}

	let key = hmac::Key::new(hmac::HMAC_SHA256, secret.trim().as_bytes());
	let payload = format!("monochange-ci-auth:v1:{normalized_repo_id}:{expected_repo_hash}");
	let tag_bytes = hex_decode(token_hmac_hex).map_err(|_| {
		MonochangeError::Config(
			"invalid CI auth token signature encoding: expected hexadecimal HMAC".to_string(),
		)
	})?;

	hmac::verify(&key, payload.as_bytes(), &tag_bytes).map_err(|_| {
		MonochangeError::Config(format!(
			"unauthorized CI auth token: HMAC signature verification failed for repository `{normalized_repo_id}`"
		))
	})
}

fn generate_random_secret_hex() -> MonochangeResult<String> {
	let rng = SystemRandom::new();
	let mut bytes = [0_u8; 32];
	rng.fill(&mut bytes).map_err(|_| {
		MonochangeError::Io("failed to generate cryptographic secret for CI auth".to_string())
	})?;
	Ok(hex_encode(&bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
	let mut output = String::with_capacity(bytes.len() * 2);
	for byte in bytes {
		let _ = write!(output, "{byte:02x}");
	}
	output
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, String> {
	if !hex.len().is_multiple_of(2) {
		return Err("odd hex length".to_string());
	}
	let mut bytes = Vec::with_capacity(hex.len() / 2);
	let chars = hex.as_bytes();
	let mut index = 0;
	while index < chars.len() {
		let pair =
			std::str::from_utf8(chars.get(index..index + 2).ok_or("slice")?).map_err(|_| "utf8")?;
		let value = u8::from_str_radix(pair, 16).map_err(|_| "invalid hex")?;
		bytes.push(value);
		index += 2;
	}
	Ok(bytes)
}

fn ci_auth_file_path(root: &Path) -> PathBuf {
	root.join(CI_AUTH_RELATIVE_PATH)
}

fn ci_memory_file_path(root: &Path) -> PathBuf {
	root.join(CI_MEMORY_RELATIVE_PATH)
}

fn load_ci_auth_file(root: &Path) -> MonochangeResult<Option<CiAuthFile>> {
	let path = ci_auth_file_path(root);
	if !path.exists() {
		return Ok(None);
	}
	let content = fs::read_to_string(&path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	let parsed: CiAuthFile = serde_json::from_str(&content).map_err(|error| {
		MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
	})?;
	Ok(Some(parsed))
}

fn save_ci_auth_file(root: &Path, auth: &CiAuthFile) -> MonochangeResult<PathBuf> {
	let path = ci_auth_file_path(root);
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}
	let serialized = serde_json::to_string_pretty(auth).map_err(|error| {
		MonochangeError::Io(format!("failed to serialize CI auth file: {error}"))
	})?;
	fs::write(&path, format!("{serialized}\n")).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let permissions = fs::Permissions::from_mode(0o600);
		fs::set_permissions(&path, permissions).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to set 0600 permissions on {}: {error}",
				path.display()
			))
		})?;
	}

	Ok(path)
}

/// Authenticate a caller against the current repository's canonical identity and secret.
pub fn authenticate_repo_access(
	root: &Path,
	repo_id_override: Option<&str>,
	explicit_token: Option<&str>,
) -> MonochangeResult<CiAuthReport> {
	let repo_id = derive_canonical_repo_id(root, repo_id_override);
	let repo_hash = compute_repo_hash(&repo_id);
	let auth_file_path = ci_auth_file_path(root);
	let auth_file = load_ci_auth_file(root)?;

	let env_secret = std::env::var("MONOCHANGE_CI_SECRET")
		.ok()
		.map(|v| v.trim().to_string())
		.filter(|v| !v.is_empty());
	let env_token = std::env::var("MONOCHANGE_CI_TOKEN")
		.ok()
		.map(|v| v.trim().to_string())
		.filter(|v| !v.is_empty());

	// verify auth_file repo_id if present
	if let Some(ref file_data) = auth_file {
		let file_repo_id = normalize_repo_identity(&file_data.repo_id);
		if file_repo_id != repo_id {
			return Err(MonochangeError::Config(format!(
				"repository authentication failed: {} belongs to `{file_repo_id}`, which does not match current repository `{repo_id}`",
				auth_file_path.display()
			)));
		}
	}

	let secret = env_secret
		.clone()
		.or_else(|| auth_file.as_ref().map(|f| f.secret.clone()))
		.ok_or_else(|| {
			MonochangeError::Config(format!(
				"repository CI authentication is not initialized for `{repo_id}`; run `monochange ci auth init` or set `MONOCHANGE_CI_SECRET` / `MONOCHANGE_CI_TOKEN`"
			))
		})?;

	let (token, source) = if let Some(tok) = explicit_token
		.map(str::trim)
		.filter(|value| !value.is_empty())
	{
		(tok.to_string(), "flag".to_string())
	} else if let Some(tok) = env_token {
		(tok, "MONOCHANGE_CI_TOKEN".to_string())
	} else if let Some(ref file_data) = auth_file {
		(file_data.token.clone(), CI_AUTH_RELATIVE_PATH.to_string())
	} else {
		(
			issue_repo_token(&repo_id, &secret),
			"MONOCHANGE_CI_SECRET".to_string(),
		)
	};

	verify_repo_token(&token, &repo_id, &secret)?;

	Ok(CiAuthReport {
		action: "verify".to_string(),
		authenticated: true,
		repo_id,
		repo_hash,
		token,
		auth_file: auth_file_path.display().to_string(),
		source,
	})
}

fn should_enforce_auth_for_context(
	root: &Path,
	require_auth_flag: bool,
	explicit_token: Option<&str>,
) -> bool {
	require_auth_flag
		|| explicit_token.is_some()
		|| std::env::var("MONOCHANGE_CI_TOKEN").is_ok_and(|v| !v.trim().is_empty())
		|| ci_auth_file_path(root).exists()
}

fn load_ci_memory_store(
	root: &Path,
	repo_id_override: Option<&str>,
	require_auth: bool,
	explicit_token: Option<&str>,
) -> MonochangeResult<CiMemoryStore> {
	let repo_id = derive_canonical_repo_id(root, repo_id_override);
	let repo_hash = compute_repo_hash(&repo_id);

	if should_enforce_auth_for_context(root, require_auth, explicit_token) {
		authenticate_repo_access(root, repo_id_override, explicit_token)?;
	}

	let path = ci_memory_file_path(root);
	if !path.exists() {
		return Ok(CiMemoryStore::new(repo_id));
	}

	let content = fs::read_to_string(&path).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
	})?;
	let store: CiMemoryStore = serde_json::from_str(&content).map_err(|error| {
		MonochangeError::Config(format!("failed to parse {}: {error}", path.display()))
	})?;

	let stored_repo_id = normalize_repo_identity(&store.repo_id);
	if stored_repo_id != repo_id || store.repo_hash != repo_hash {
		return Err(MonochangeError::Config(format!(
			"CI memory repository mismatch: {} belongs to `{}` (`{}`), but current repository is `{repo_id}` (`{repo_hash}`)",
			path.display(),
			store.repo_id,
			store.repo_hash
		)));
	}

	Ok(store)
}

fn save_ci_memory_store(root: &Path, store: &CiMemoryStore) -> MonochangeResult<PathBuf> {
	let path = ci_memory_file_path(root);
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}
	let serialized = serde_json::to_string_pretty(store).map_err(|error| {
		MonochangeError::Io(format!("failed to serialize CI memory store: {error}"))
	})?;
	fs::write(&path, format!("{serialized}\n")).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;
	Ok(path)
}

fn run_ci_auth_subcommand(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	match matches.subcommand() {
		Some(("init", sub_matches)) => {
			let repo_id_override = sub_matches.get_one::<String>("repo-id").map(String::as_str);
			let force = sub_matches.get_flag("force");
			let format = sub_matches
				.get_one::<String>("format")
				.map_or("text", String::as_str);

			let auth_path = ci_auth_file_path(root);
			if auth_path.exists() && !force {
				return Err(MonochangeError::Config(format!(
					"{} already exists; pass `--force` to regenerate the repository secret and token",
					auth_path.display()
				)));
			}

			let repo_id = derive_canonical_repo_id(root, repo_id_override);
			let repo_hash = compute_repo_hash(&repo_id);
			let secret = std::env::var("MONOCHANGE_CI_SECRET")
				.ok()
				.map(|value| value.trim().to_string())
				.filter(|value| !value.is_empty())
				.map_or_else(generate_random_secret_hex, Ok)?;
			let token = issue_repo_token(&repo_id, &secret);
			let auth_file = CiAuthFile {
				schema_version: 1,
				repo_id: repo_id.clone(),
				repo_hash: repo_hash.clone(),
				secret,
				token: token.clone(),
				created_at: Utc::now().to_rfc3339(),
			};
			let saved_path = save_ci_auth_file(root, &auth_file)?;
			let report = CiAuthReport {
				action: "init".to_string(),
				authenticated: true,
				repo_id,
				repo_hash,
				token,
				auth_file: saved_path.display().to_string(),
				source: CI_AUTH_RELATIVE_PATH.to_string(),
			};

			render_auth_report(&report, format, false)
		}
		Some(("verify", sub_matches)) => {
			let repo_id_override = sub_matches.get_one::<String>("repo-id").map(String::as_str);
			let token = sub_matches.get_one::<String>("token").map(String::as_str);
			let format = sub_matches
				.get_one::<String>("format")
				.map_or("text", String::as_str);

			let report = authenticate_repo_access(root, repo_id_override, token)?;
			render_auth_report(&report, format, false)
		}
		Some(("token", sub_matches)) => {
			let repo_id_override = sub_matches.get_one::<String>("repo-id").map(String::as_str);
			let format = sub_matches
				.get_one::<String>("format")
				.map_or("text", String::as_str);

			let mut report = authenticate_repo_access(root, repo_id_override, None)?;
			report.action = "token".to_string();
			render_auth_report(&report, format, true)
		}
		_ => {
			Err(MonochangeError::Config(
				"missing `monochange ci auth` subcommand".to_string(),
			))
		}
	}
}

fn render_auth_report(
	report: &CiAuthReport,
	format: &str,
	raw_token_in_text: bool,
) -> MonochangeResult<String> {
	if format == "json" {
		return serde_json::to_string_pretty(report)
			.map_err(|error| MonochangeError::Io(format!("failed to render auth JSON: {error}")));
	}

	if raw_token_in_text {
		return Ok(report.token.clone());
	}

	Ok(format!(
		"action: {}\nauthenticated: {}\nrepo_id: {}\nrepo_hash: {}\ntoken: {}\nauth_file: {}\nsource: {}",
		report.action,
		report.authenticated,
		report.repo_id,
		report.repo_hash,
		report.token,
		report.auth_file,
		report.source
	))
}

#[derive(Debug, Clone)]
struct WorkspaceGraphContext {
	affected_package_ids: BTreeSet<String>,
	package_names_to_ids: BTreeMap<String, String>,
	docs_or_ignored_only: bool,
}

fn evaluate_workspace_graph_context(
	root: &Path,
	changed_files: &[String],
) -> WorkspaceGraphContext {
	let normalized_files = changed_files
		.iter()
		.map(|path| normalize_changed_path(path))
		.filter(|path| !path.is_empty())
		.collect::<Vec<_>>();

	if normalized_files.is_empty() {
		return WorkspaceGraphContext {
			affected_package_ids: BTreeSet::new(),
			package_names_to_ids: BTreeMap::new(),
			docs_or_ignored_only: true,
		};
	}

	let all_docs_or_non_code = normalized_files
		.iter()
		.all(|path| is_doc_or_metadata_path(path));

	let mut direct_affected = BTreeSet::new();
	let mut package_names_to_ids = BTreeMap::new();

	if let Ok(configuration) = load_workspace_configuration(root) {
		for package in &configuration.packages {
			package_names_to_ids.insert(package.id.clone(), package.id.clone());
			let matcher = PackagePathMatcher::new(
				&package.id,
				&package.path,
				&package.additional_paths,
				&package.ignored_paths,
			);
			for file in &normalized_files {
				if matches!(matcher.classify(Path::new(file)), PackagePathMatch::Touched) {
					direct_affected.insert(package.id.clone());
				}
			}
		}
	}

	if let Ok(discovery) = discover_workspace(root) {
		for record in &discovery.packages {
			let config_id = record
				.metadata
				.get("config_id")
				.cloned()
				.unwrap_or_else(|| record.id.clone());
			package_names_to_ids.insert(record.name.clone(), config_id.clone());
			package_names_to_ids.insert(record.id.clone(), config_id.clone());
			let rel_dir = record
				.manifest_path
				.parent()
				.and_then(|dir| dir.strip_prefix(&discovery.workspace_root).ok())
				.map(|dir| dir.to_string_lossy().replace('\\', "/"))
				.unwrap_or_default();

			for file in &normalized_files {
				if !rel_dir.is_empty()
					&& (file == &rel_dir || file.starts_with(&format!("{rel_dir}/")))
					&& !is_doc_or_metadata_path(file)
				{
					direct_affected.insert(config_id.clone());
				}
			}
		}

		// Propagate transitive dependents: if A depends on B and B is affected, A is affected
		let edges = materialize_dependency_edges(&discovery.packages);
		let mut changed = true;
		while changed {
			changed = false;
			for edge in &edges {
				let from_id = package_names_to_ids
					.get(&edge.from_package_id)
					.cloned()
					.unwrap_or_else(|| edge.from_package_id.clone());
				let to_id = package_names_to_ids
					.get(&edge.to_package_id)
					.cloned()
					.unwrap_or_else(|| edge.to_package_id.clone());
				if direct_affected.contains(&to_id) && direct_affected.insert(from_id) {
					changed = true;
				}
			}
		}
	}

	WorkspaceGraphContext {
		docs_or_ignored_only: all_docs_or_non_code && direct_affected.is_empty(),
		affected_package_ids: direct_affected,
		package_names_to_ids,
	}
}

fn is_doc_or_metadata_path(path: &str) -> bool {
	let lower = path.to_ascii_lowercase();
	let ext = Path::new(path).extension().and_then(|e| e.to_str());
	ext.is_some_and(|e| {
		e.eq_ignore_ascii_case("md")
			|| e.eq_ignore_ascii_case("mdx")
			|| e.eq_ignore_ascii_case("txt")
	}) || lower.starts_with("docs/")
		|| lower.starts_with(".changeset/")
		|| lower.starts_with(".monochange/")
		|| lower == "license"
		|| lower == ".gitignore"
}

fn is_rust_job_or_context(job: &str, changed_files: &[String], root: &Path) -> bool {
	let lower = job.to_ascii_lowercase();
	if lower.contains("rust")
		|| lower.contains("cargo")
		|| lower.contains("clippy")
		|| lower.contains("nextest")
		|| lower.contains("miri")
		|| lower.contains("semver")
	{
		return true;
	}
	if changed_files.iter().any(|file| {
		Path::new(file)
			.extension()
			.is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
			|| file.ends_with("Cargo.toml")
			|| file == "Cargo.lock"
	}) {
		return true;
	}
	root.join("Cargo.toml").is_file() && lower.contains("test")
}

pub fn build_speed_suggestions(
	job: &str,
	is_rust: bool,
	duration_seconds: u64,
	threshold_minutes: u64,
) -> Vec<String> {
	let duration_minutes = duration_seconds as f64 / 60.0;
	let mut suggestions = vec![format!(
		"Job `{job}` took {duration_minutes:.1}m, exceeding the {threshold_minutes}m threshold."
	)];
	if is_rust {
		suggestions.push(
			"Enable `sccache` or `Swatinem/rust-cache` to share compiled crate artifacts across CI runs."
				.to_string(),
		);
		suggestions.push(
			"Switch test execution to `cargo-nextest` and shard slow integration suites across matrix runners."
				.to_string(),
		);
		suggestions.push(
			"Use a faster linker (`mold` on Linux or `lld`) and scope `cargo test -p <crate>` to affected workspace packages."
				.to_string(),
		);
	} else {
		suggestions.push(
			"Enable dependency and build caching for this job and parallelize independent test shards."
				.to_string(),
		);
		suggestions.push(
			"Scope the job command to only affected workspace packages or directories rather than running the full workspace."
				.to_string(),
		);
	}
	suggestions
}

fn detect_changed_files_from_git(
	root: &Path,
	base: Option<&str>,
	head: Option<&str>,
) -> Vec<String> {
	let mut args = vec!["diff".to_string(), "--name-only".to_string()];
	match (base, head) {
		(Some(b), Some(h)) => args.push(format!("{b}...{h}")),
		(Some(b), None) => args.push(b.to_string()),
		(None, Some(h)) => args.push(h.to_string()),
		(None, None) => args.push("HEAD".to_string()),
	}
	let output = ProcessCommand::new("git")
		.current_dir(root)
		.args(&args)
		.output();
	let Ok(output) = output else {
		return Vec::new();
	};
	if !output.status.success() {
		return Vec::new();
	}
	String::from_utf8_lossy(&output.stdout)
		.lines()
		.map(normalize_changed_path)
		.filter(|line| !line.is_empty())
		.collect()
}

fn matches_any_glob(value: &str, patterns: &[String]) -> Option<String> {
	for pattern_str in patterns {
		if let Ok(pattern) = Pattern::new(pattern_str)
			&& pattern.matches(value)
		{
			return Some(pattern_str.clone());
		}
		if value == pattern_str {
			return Some(pattern_str.clone());
		}
	}
	None
}

fn is_main_branch(branch: Option<&str>, git_ref: Option<&str>) -> bool {
	if let Some(b) = branch {
		let trimmed = b.trim();
		if trimmed == "main"
			|| trimmed == "master"
			|| trimmed == "refs/heads/main"
			|| trimmed == "refs/heads/master"
		{
			return true;
		}
	}
	if let Some(r) = git_ref {
		let trimmed = r.trim();
		if trimmed == "main"
			|| trimmed == "master"
			|| trimmed == "refs/heads/main"
			|| trimmed == "refs/heads/master"
		{
			return true;
		}
	}
	false
}

#[allow(clippy::too_many_arguments)]
fn evaluate_graph_decision_for_job(
	job: &str,
	changed_files: &[String],
	job_paths: &[String],
	job_packages: &[String],
	graph: &WorkspaceGraphContext,
) -> (bool, String, Vec<String>) {
	if changed_files.is_empty() {
		return (
			false,
			format!("no changed files detected for job `{job}`"),
			Vec::new(),
		);
	}

	if !job_paths.is_empty() {
		for file in changed_files {
			if let Some(matched_glob) = matches_any_glob(file, job_paths) {
				return (
					true,
					format!("changed file `{file}` matched job path pattern `{matched_glob}`"),
					graph.affected_package_ids.iter().cloned().collect(),
				);
			}
		}
		return (
			false,
			format!("no changed files matched configured `--job-paths` for `{job}`"),
			Vec::new(),
		);
	}

	if !job_packages.is_empty() {
		let mut matched = Vec::new();
		for requested in job_packages {
			let resolved = graph
				.package_names_to_ids
				.get(requested)
				.cloned()
				.unwrap_or_else(|| requested.clone());
			if graph.affected_package_ids.contains(&resolved) {
				matched.push(resolved);
			}
		}
		if !matched.is_empty() {
			return (
				true,
				format!(
					"affected workspace package(s) `{}` matched job `{job}`",
					matched.join(", ")
				),
				matched,
			);
		}
		return (
			false,
			format!(
				"none of the job packages ({}) were affected",
				job_packages.join(", ")
			),
			Vec::new(),
		);
	}

	if graph.docs_or_ignored_only {
		let lower_job = job.to_ascii_lowercase();
		if lower_job.contains("doc")
			|| lower_job.contains("spell")
			|| lower_job.contains("markdown")
		{
			return (
				true,
				format!("documentation job `{job}` matched documentation-only changes"),
				Vec::new(),
			);
		}
		return (
			false,
			format!(
				"skipping `{job}` because only documentation or non-code metadata files changed"
			),
			Vec::new(),
		);
	}

	// Check if job name specifically names a known package in the workspace
	let mut job_named_packages = Vec::new();
	for (pkg_name, pkg_id) in &graph.package_names_to_ids {
		if job == pkg_name
			|| job == pkg_id
			|| job.starts_with(&format!("{pkg_name}:"))
			|| job.ends_with(&format!(":{pkg_name}"))
			|| job.starts_with(&format!("{pkg_id}:"))
			|| job.ends_with(&format!(":{pkg_id}"))
		{
			job_named_packages.push(pkg_id.clone());
		}
	}
	job_named_packages.sort();
	job_named_packages.dedup();

	if !job_named_packages.is_empty() {
		let affected_for_job: Vec<String> = job_named_packages
			.into_iter()
			.filter(|id| graph.affected_package_ids.contains(id))
			.collect();
		if affected_for_job.is_empty() {
			return (
				false,
				format!("package-scoped job `{job}` was not affected by the changed files"),
				Vec::new(),
			);
		}
		return (
			true,
			format!(
				"package-scoped job `{job}` affects package(s): {}",
				affected_for_job.join(", ")
			),
			affected_for_job,
		);
	}

	if !graph.affected_package_ids.is_empty() {
		let affected: Vec<String> = graph.affected_package_ids.iter().cloned().collect();
		return (
			true,
			format!(
				"workspace graph detected {} affected package(s): {}",
				affected.len(),
				affected.join(", ")
			),
			affected,
		);
	}

	(
		true,
		format!("non-documentation files changed; running `{job}` conservatively"),
		Vec::new(),
	)
}

struct ProviderCallOutcome {
	run: bool,
	reason: String,
	provider_used: String,
	model_used: String,
}

#[allow(clippy::too_many_arguments)]
async fn query_external_decision_provider(
	provider: CiDecisionProvider,
	model_override: Option<&str>,
	api_url_override: Option<&str>,
	api_key_override: Option<&str>,
	account_id_override: Option<&str>,
	job: &str,
	changed_files: &[String],
	affected_packages: &[String],
) -> Result<ProviderCallOutcome, String> {
	let _ = rustls::crypto::ring::default_provider().install_default();
	let client = Client::builder()
		.connect_timeout(Duration::from_secs(5))
		.timeout(Duration::from_secs(15))
		.build()
		.map_err(|error| format!("failed to build HTTP client: {error}"))?;

	match provider {
		CiDecisionProvider::Graph => Err("graph provider is local".to_string()),
		CiDecisionProvider::Jev => {
			let env_url = std::env::var("JEV_API_URL").ok();
			let url = api_url_override
				.map(str::trim)
				.filter(|v| !v.is_empty())
				.or_else(|| env_url.as_deref().map(str::trim).filter(|v| !v.is_empty()))
				.ok_or_else(|| "missing JEV_API_URL or --api-url".to_string())?;
			let api_key = api_key_override
				.map(ToString::to_string)
				.or_else(|| std::env::var("JEV_API_KEY").ok());

			let mut req = client.post(url).json(&json!({
				"job": job,
				"changed_files": changed_files,
				"affected_packages": affected_packages,
			}));
			if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
				req = req.bearer_auth(key.trim());
			}
			let response = req
				.send()
				.await
				.map_err(|error| format!("jev webhook request failed: {error}"))?;
			if !response.status().is_success() {
				return Err(format!("jev webhook returned HTTP {}", response.status()));
			}
			let text = response
				.text()
				.await
				.map_err(|error| format!("failed reading jev response: {error}"))?;
			let (run, reason) = parse_provider_decision_payload(&text)?;
			Ok(ProviderCallOutcome {
				run,
				reason,
				provider_used: "jev".to_string(),
				model_used: model_override.unwrap_or("jev-webhook").to_string(),
			})
		}
		CiDecisionProvider::OpencodeGo
		| CiDecisionProvider::Openrouter
		| CiDecisionProvider::Cloudflare
		| CiDecisionProvider::OpenaiCompatible => {
			let (endpoint, api_key, default_model) = match provider {
				CiDecisionProvider::OpencodeGo => {
					let url = api_url_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("OPENCODE_GO_API_URL").ok())
						.unwrap_or_else(|| DEFAULT_OPENCODE_GO_API_URL.to_string());
					let key = api_key_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("OPENCODE_GO_API_KEY").ok())
						.ok_or_else(|| "missing OPENCODE_GO_API_KEY or --api-key".to_string())?;
					(url, key, DEFAULT_OPENCODE_GO_MODEL)
				}
				CiDecisionProvider::Openrouter => {
					let url = api_url_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("OPENROUTER_API_URL").ok())
						.unwrap_or_else(|| DEFAULT_OPENROUTER_API_URL.to_string());
					let key = api_key_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
						.ok_or_else(|| "missing OPENROUTER_API_KEY or --api-key".to_string())?;
					(url, key, DEFAULT_OPENROUTER_MODEL)
				}
				CiDecisionProvider::Cloudflare => {
					let account_id = account_id_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("CLOUDFLARE_ACCOUNT_ID").ok())
						.unwrap_or_else(|| "default".to_string());
					let url = api_url_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("CLOUDFLARE_API_URL").ok())
						.unwrap_or_else(|| {
							format!(
								"https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/v1/chat/completions"
							)
						});
					let key = api_key_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("CLOUDFLARE_API_TOKEN").ok())
						.ok_or_else(|| "missing CLOUDFLARE_API_TOKEN or --api-key".to_string())?;
					(url, key, DEFAULT_CLOUDFLARE_MODEL)
				}
				CiDecisionProvider::OpenaiCompatible => {
					let url = api_url_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("OPENAI_API_URL").ok())
						.ok_or_else(|| {
							"missing --api-url or OPENAI_API_URL for openai-compatible".to_string()
						})?;
					let key = api_key_override
						.map(ToString::to_string)
						.or_else(|| std::env::var("OPENAI_API_KEY").ok())
						.unwrap_or_default();
					(url, key, DEFAULT_OPENCODE_GO_MODEL)
				}
				_ => unreachable!(),
			};

			let model = model_override.unwrap_or(default_model).to_string();
			let prompt = format!(
				"Decide if CI job `{job}` must run (`y` or `n`). Changed files: {changed_files:?}. Affected workspace packages: {affected_packages:?}. Reply with JSON {{\"decision\":\"y\"|\"n\",\"reason\":\"...\"}}."
			);
			let mut req = client.post(&endpoint).json(&json!({
				"model": model,
				"temperature": 0.0,
				"messages": [
					{
						"role": "system",
						"content": "You are monochange's CI gatekeeper. Respond strictly with JSON: {\"decision\": \"y\" | \"n\", \"reason\": \"...\"}."
					},
					{
						"role": "user",
						"content": prompt
					}
				]
			}));
			if !api_key.trim().is_empty() {
				req = req.bearer_auth(api_key.trim());
			}
			let response = req
				.send()
				.await
				.map_err(|error| format!("{} request failed: {error}", provider.as_str()))?;
			if !response.status().is_success() {
				return Err(format!(
					"{} returned HTTP {}",
					provider.as_str(),
					response.status()
				));
			}
			let body_text = response
				.text()
				.await
				.map_err(|error| format!("failed reading provider response: {error}"))?;
			let (run, reason) = parse_provider_decision_payload(&body_text)?;
			Ok(ProviderCallOutcome {
				run,
				reason,
				provider_used: provider.as_str().to_string(),
				model_used: model,
			})
		}
	}
}

fn parse_provider_decision_payload(payload: &str) -> Result<(bool, String), String> {
	let trimmed = payload.trim();
	if trimmed.eq_ignore_ascii_case("y")
		|| trimmed.eq_ignore_ascii_case("yes")
		|| trimmed.eq_ignore_ascii_case("true")
	{
		return Ok((true, "provider returned `y`".to_string()));
	}
	if trimmed.eq_ignore_ascii_case("n")
		|| trimmed.eq_ignore_ascii_case("no")
		|| trimmed.eq_ignore_ascii_case("false")
	{
		return Ok((false, "provider returned `n`".to_string()));
	}

	let value: Value =
		serde_json::from_str(trimmed).map_err(|e| format!("invalid JSON response: {e}"))?;
	if let Some(content) = value
		.pointer("/choices/0/message/content")
		.and_then(Value::as_str)
	{
		return parse_provider_decision_payload(content);
	}

	let decision_opt = value
		.get("decision")
		.and_then(Value::as_str)
		.map(|s| {
			s.eq_ignore_ascii_case("y")
				|| s.eq_ignore_ascii_case("yes")
				|| s.eq_ignore_ascii_case("true")
				|| s.eq_ignore_ascii_case("run")
		})
		.or_else(|| value.get("run").and_then(Value::as_bool));

	let Some(run) = decision_opt else {
		return Err(
			"provider response did not include `decision` (`y`/`n`) or `run` boolean".to_string(),
		);
	};
	let reason = value
		.get("reason")
		.and_then(Value::as_str)
		.unwrap_or(if run {
			"provider decided to run job"
		} else {
			"provider decided to skip job"
		})
		.to_string();
	Ok((run, reason))
}

async fn run_ci_decide_subcommand(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let repo_id_override = matches.get_one::<String>("repo-id").map(String::as_str);
	let require_auth = matches.get_flag("require-auth");
	let explicit_token = matches.get_one::<String>("token").map(String::as_str);
	let mut memory = load_ci_memory_store(root, repo_id_override, require_auth, explicit_token)?;

	let jobs: Vec<String> = matches.get_many::<String>("job").map_or_else(
		|| vec!["default".to_string()],
		|vals| vals.cloned().collect(),
	);

	let provider_str = matches
		.get_one::<String>("provider")
		.map_or("opencode-go", String::as_str);
	let provider = CiDecisionProvider::parse(provider_str)?;
	let model_override = matches.get_one::<String>("model").map(String::as_str);
	let api_url_override = matches.get_one::<String>("api-url").map(String::as_str);
	let api_key_override = matches.get_one::<String>("api-key").map(String::as_str);
	let account_id_override = matches.get_one::<String>("account-id").map(String::as_str);

	let explicit_changed_files: Vec<String> = matches
		.get_many::<String>("changed-files")
		.map(|vals| {
			vals.map(|s| normalize_changed_path(s))
				.filter(|s| !s.is_empty())
				.collect()
		})
		.unwrap_or_default();
	let base_ref = matches.get_one::<String>("base").map(String::as_str);
	let head_ref = matches.get_one::<String>("head").map(String::as_str);
	let changed_files =
		if explicit_changed_files.is_empty() && (base_ref.is_some() || head_ref.is_some()) {
			detect_changed_files_from_git(root, base_ref, head_ref)
		} else {
			explicit_changed_files
		};

	let event = matches.get_one::<String>("event").cloned().or_else(|| {
		std::env::var("GITHUB_EVENT_NAME")
			.ok()
			.filter(|v| !v.is_empty())
	});
	let branch = matches.get_one::<String>("branch").cloned().or_else(|| {
		std::env::var("GITHUB_REF_NAME")
			.ok()
			.filter(|v| !v.is_empty())
	});
	let git_ref = matches
		.get_one::<String>("ref")
		.cloned()
		.or_else(|| std::env::var("GITHUB_REF").ok().filter(|v| !v.is_empty()));
	let commit = matches
		.get_one::<String>("commit")
		.cloned()
		.or_else(|| std::env::var("GITHUB_SHA").ok().filter(|v| !v.is_empty()));
	let commit_message = matches.get_one::<String>("commit-message").cloned();
	let labels: Vec<String> = matches
		.get_many::<String>("labels")
		.map(|vals| vals.cloned().collect())
		.unwrap_or_default();

	let unconditional_flag = matches.get_flag("unconditional");
	let mut unconditional_events: Vec<String> = DEFAULT_UNCONDITIONAL_EVENTS
		.iter()
		.map(|s| (*s).to_string())
		.collect();
	if let Some(extra) = matches.get_many::<String>("unconditional-events") {
		unconditional_events.extend(extra.cloned());
	}

	let mut unconditional_branches: Vec<String> = DEFAULT_UNCONDITIONAL_BRANCHES
		.iter()
		.map(|s| (*s).to_string())
		.collect();
	if let Some(extra) = matches.get_many::<String>("unconditional-branches") {
		unconditional_branches.extend(extra.cloned());
	}

	let mut unconditional_paths: Vec<String> = DEFAULT_UNCONDITIONAL_PATHS
		.iter()
		.map(|s| (*s).to_string())
		.collect();
	if let Some(extra) = matches.get_many::<String>("unconditional-paths") {
		unconditional_paths.extend(extra.cloned());
	}

	let max_skips_on_main = matches
		.get_one::<u32>("max-skips-on-main")
		.copied()
		.unwrap_or(DEFAULT_MAX_SKIPS_ON_MAIN);
	let explicit_duration = matches.get_one::<u64>("duration-seconds").copied();
	let max_duration_minutes = matches
		.get_one::<u64>("max-duration-minutes")
		.copied()
		.unwrap_or(DEFAULT_MAX_DURATION_MINUTES);
	let rust_max_duration_minutes = matches
		.get_one::<u64>("rust-max-duration-minutes")
		.copied()
		.unwrap_or(DEFAULT_RUST_MAX_DURATION_MINUTES);

	let job_paths: Vec<String> = matches
		.get_many::<String>("job-paths")
		.map(|vals| vals.cloned().collect())
		.unwrap_or_default();
	let job_packages: Vec<String> = matches
		.get_many::<String>("job-packages")
		.map(|vals| vals.cloned().collect())
		.unwrap_or_default();

	let graph_ctx = evaluate_workspace_graph_context(root, &changed_files);
	let on_main = is_main_branch(branch.as_deref(), git_ref.as_deref());

	// Evaluate global unconditional overrides
	let mut base_unconditional_reasons = Vec::new();
	if unconditional_flag {
		base_unconditional_reasons.push("explicit `--unconditional` flag".to_string());
	}
	if let Some(ref ev) = event
		&& unconditional_events
			.iter()
			.any(|candidate| candidate.eq_ignore_ascii_case(ev))
	{
		base_unconditional_reasons.push(format!("event `{ev}` matches unconditional events"));
	}
	if let Some(ref br) = branch
		&& let Some(matched) = matches_any_glob(br, &unconditional_branches)
	{
		base_unconditional_reasons.push(format!(
			"branch `{br}` matches unconditional branch pattern `{matched}`"
		));
	}
	if let Some(ref rf) = git_ref
		&& let Some(matched) = matches_any_glob(rf, &unconditional_branches)
	{
		base_unconditional_reasons.push(format!(
			"ref `{rf}` matches unconditional branch/ref pattern `{matched}`"
		));
	}
	for label in &labels {
		let lower = label.to_ascii_lowercase();
		if UNCONDITIONAL_MARKERS
			.iter()
			.any(|marker| lower.contains(marker))
		{
			base_unconditional_reasons.push(format!("label `{label}` forces unconditional CI"));
		}
	}
	if let Some(ref msg) = commit_message {
		let lower = msg.to_ascii_lowercase();
		if let Some(marker) = UNCONDITIONAL_MARKERS
			.iter()
			.find(|marker| lower.contains(**marker))
		{
			base_unconditional_reasons.push(format!(
				"commit message contains unconditional marker `{marker}`"
			));
		}
	}
	for file in &changed_files {
		if let Some(matched) = matches_any_glob(file, &unconditional_paths) {
			base_unconditional_reasons.push(format!(
				"changed file `{file}` matches unconditional path pattern `{matched}`"
			));
		}
	}

	let mut job_records = Vec::with_capacity(jobs.len());

	for job in &jobs {
		let mut job_unconditional_reasons = base_unconditional_reasons.clone();
		let mut matched_learned_rules = Vec::new();

		for rule in &memory.learned_rules {
			if rule.job == "*" || rule.job == *job {
				for file in &changed_files {
					if matches_any_glob(file, &rule.patterns).is_some() {
						matched_learned_rules.push(rule.id.clone());
						job_unconditional_reasons.push(format!(
							"learned rule `{}` (from past false_negative) matched `{file}`",
							rule.id
						));
						break;
					}
				}
			}
		}

		let prior_skips = memory
			.consecutive_skips_on_main
			.get(job)
			.copied()
			.unwrap_or(0);
		if on_main && prior_skips >= max_skips_on_main {
			job_unconditional_reasons.push(format!(
				"consecutive skips on main ({prior_skips}) reached `--max-skips-on-main` ({max_skips_on_main})"
			));
		}

		let unconditional = !job_unconditional_reasons.is_empty();
		let (graph_run, graph_reason, affected_packages) = evaluate_graph_decision_for_job(
			job,
			&changed_files,
			&job_paths,
			&job_packages,
			&graph_ctx,
		);

		let (run, reason, provider_used, model_used) = if unconditional {
			let default_model = model_override
				.unwrap_or(match provider {
					CiDecisionProvider::OpencodeGo | CiDecisionProvider::OpenaiCompatible => {
						DEFAULT_OPENCODE_GO_MODEL
					}
					CiDecisionProvider::Openrouter => DEFAULT_OPENROUTER_MODEL,
					CiDecisionProvider::Cloudflare => DEFAULT_CLOUDFLARE_MODEL,
					CiDecisionProvider::Jev => "jev-webhook",
					CiDecisionProvider::Graph => "workspace-graph",
				})
				.to_string();
			(
				true,
				format!(
					"forced unconditional run (`y`): {}",
					job_unconditional_reasons.join("; ")
				),
				provider.as_str().to_string(),
				default_model,
			)
		} else if provider == CiDecisionProvider::Graph {
			(
				graph_run,
				graph_reason,
				"graph".to_string(),
				model_override.unwrap_or("workspace-graph").to_string(),
			)
		} else {
			match query_external_decision_provider(
				provider,
				model_override,
				api_url_override,
				api_key_override,
				account_id_override,
				job,
				&changed_files,
				&affected_packages,
			)
			.await
			{
				Ok(outcome) => {
					(
						outcome.run,
						outcome.reason,
						outcome.provider_used,
						outcome.model_used,
					)
				}
				Err(_fallback_err) => {
					let configured_model = model_override
						.unwrap_or(match provider {
							CiDecisionProvider::OpencodeGo
							| CiDecisionProvider::OpenaiCompatible => DEFAULT_OPENCODE_GO_MODEL,
							CiDecisionProvider::Openrouter => DEFAULT_OPENROUTER_MODEL,
							CiDecisionProvider::Cloudflare => DEFAULT_CLOUDFLARE_MODEL,
							CiDecisionProvider::Jev => "jev-webhook",
							CiDecisionProvider::Graph => "workspace-graph",
						})
						.to_string();
					(
						graph_run,
						format!("{graph_reason} (evaluated via deterministic graph fallback)"),
						"graph".to_string(),
						configured_model,
					)
				}
			}
		};

		let updated_skips = if on_main {
			if run {
				memory.consecutive_skips_on_main.insert(job.clone(), 0);
				0
			} else {
				let next = prior_skips + 1;
				memory.consecutive_skips_on_main.insert(job.clone(), next);
				next
			}
		} else {
			prior_skips
		};

		// Check duration history or explicit duration against red-flag threshold
		let is_rust = is_rust_job_or_context(job, &changed_files, root);
		let threshold_minutes = if is_rust {
			rust_max_duration_minutes
		} else {
			max_duration_minutes
		};
		let historical_max_duration = memory
			.verifications
			.iter()
			.filter(|v| v.job == *job)
			.map(|v| v.duration_seconds)
			.max();
		let observed_duration_seconds = explicit_duration.or(historical_max_duration);
		let duration_red_flag =
			observed_duration_seconds.is_some_and(|secs| secs > threshold_minutes * 60);
		let speed_suggestions = if let Some(secs) = observed_duration_seconds
			&& duration_red_flag
		{
			build_speed_suggestions(job, is_rust, secs, threshold_minutes)
		} else {
			Vec::new()
		};

		let record = CiDecisionRecord {
			id: format!("dec-{}-{}", memory.decisions.len() + 1, unix_millis()),
			repo_id: memory.repo_id.clone(),
			job: job.clone(),
			decision: if run {
				"y".to_string()
			} else {
				"n".to_string()
			},
			run,
			unconditional,
			provider: provider_used,
			model: model_used,
			reason,
			unconditional_reasons: job_unconditional_reasons,
			matched_learned_rules,
			affected_packages,
			changed_files: changed_files.clone(),
			event: event.clone(),
			branch: branch.clone().or_else(|| git_ref.clone()),
			commit: commit.clone(),
			consecutive_skips_on_main: updated_skips,
			duration_red_flag,
			observed_duration_seconds,
			threshold_minutes,
			speed_suggestions,
			timestamp: Utc::now().to_rfc3339(),
		};

		memory.decisions.push(record.clone());
		job_records.push(record);
	}

	save_ci_memory_store(root, &memory)?;

	let overall_run = job_records.iter().any(|r| r.run);
	let overall_unconditional = job_records.iter().any(|r| r.unconditional);
	let primary = job_records.first().cloned().unwrap_or_else(|| {
		CiDecisionRecord {
			id: "dec-0".to_string(),
			repo_id: memory.repo_id.clone(),
			job: "default".to_string(),
			decision: "y".to_string(),
			run: true,
			unconditional: false,
			provider: provider.as_str().to_string(),
			model: DEFAULT_OPENCODE_GO_MODEL.to_string(),
			reason: "default".to_string(),
			unconditional_reasons: Vec::new(),
			matched_learned_rules: Vec::new(),
			affected_packages: Vec::new(),
			changed_files: Vec::new(),
			event: None,
			branch: None,
			commit: None,
			consecutive_skips_on_main: 0,
			duration_red_flag: false,
			observed_duration_seconds: None,
			threshold_minutes: max_duration_minutes,
			speed_suggestions: Vec::new(),
			timestamp: Utc::now().to_rfc3339(),
		}
	});

	let report = CiDecideReport {
		repo_id: memory.repo_id,
		decision: if overall_run {
			"y".to_string()
		} else {
			"n".to_string()
		},
		run: overall_run,
		unconditional: overall_unconditional,
		provider: primary.provider.clone(),
		model: primary.model.clone(),
		reason: primary.reason.clone(),
		jobs: job_records,
	};

	let format = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	render_decide_report(&report, format)
}

fn render_decide_report(report: &CiDecideReport, format: &str) -> MonochangeResult<String> {
	match format {
		"json" => {
			serde_json::to_string_pretty(report).map_err(|error| {
				MonochangeError::Io(format!("failed to serialize decide report: {error}"))
			})
		}
		"github-outputs" => {
			let jobs_map: BTreeMap<&str, &str> = report
				.jobs
				.iter()
				.map(|j| (j.job.as_str(), j.decision.as_str()))
				.collect();
			let jobs_json = serde_json::to_string(&jobs_map).unwrap_or_else(|_| "{}".to_string());
			let lines = [
				format!("decision={}", report.decision),
				format!("run={}", report.run),
				format!("unconditional={}", report.unconditional),
				format!("provider={}", report.provider),
				format!("model={}", report.model),
				format!("reason={}", report.reason.replace('\n', " ")),
				format!("jobs={jobs_json}"),
			];
			let output_str = lines.join("\n");
			if let Ok(github_output_path) = std::env::var("GITHUB_OUTPUT")
				&& !github_output_path.trim().is_empty()
			{
				let _ = fs::OpenOptions::new()
					.create(true)
					.append(true)
					.open(github_output_path)
					.and_then(|mut file| {
						use std::io::Write;
						writeln!(file, "{output_str}")
					});
			}
			Ok(output_str)
		}
		_ => {
			let mut lines = vec![
				format!("decision: {}", report.decision),
				format!("run: {}", report.run),
				format!("unconditional: {}", report.unconditional),
				format!("provider: {} ({})", report.provider, report.model),
				format!("reason: {}", report.reason),
			];
			for job_rec in &report.jobs {
				lines.push(format!(
					"job `{}`: {} (run={}, unconditional={}, skips_on_main={}) — {}",
					job_rec.job,
					job_rec.decision,
					job_rec.run,
					job_rec.unconditional,
					job_rec.consecutive_skips_on_main,
					job_rec.reason
				));
				if job_rec.duration_red_flag {
					for suggestion in &job_rec.speed_suggestions {
						lines.push(format!("  speed suggestion: {suggestion}"));
					}
				}
			}
			Ok(lines.join("\n"))
		}
	}
}

fn derive_learned_patterns_from_files(changed_files: &[String]) -> Vec<String> {
	let mut patterns = BTreeSet::new();
	for file in changed_files {
		let normalized = normalize_changed_path(file);
		if normalized.is_empty() {
			continue;
		}
		patterns.insert(normalized.clone());
		let segments: Vec<&str> = normalized.split('/').collect();
		if segments.len() >= 2
			&& matches!(
				segments.first().copied(),
				Some("crates" | "packages" | "apps" | "libs" | "services")
			) && let (Some(prefix), Some(name)) = (segments.first(), segments.get(1))
		{
			patterns.insert(format!("{prefix}/{name}/*"));
			patterns.insert(format!("{prefix}/{name}/**"));
		} else if let Some((parent, _)) = normalized.rsplit_once('/') {
			patterns.insert(format!("{parent}/*"));
		}
	}
	if patterns.is_empty() {
		patterns.insert("*".to_string());
	}
	patterns.into_iter().collect()
}

fn run_ci_verify_subcommand(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let repo_id_override = matches.get_one::<String>("repo-id").map(String::as_str);
	let require_auth = matches.get_flag("require-auth");
	let explicit_token = matches.get_one::<String>("token").map(String::as_str);
	let mut memory = load_ci_memory_store(root, repo_id_override, require_auth, explicit_token)?;

	let job = matches
		.get_one::<String>("job")
		.cloned()
		.ok_or_else(|| MonochangeError::Config("missing `--job`".to_string()))?;
	let conclusion = matches
		.get_one::<String>("conclusion")
		.cloned()
		.ok_or_else(|| MonochangeError::Config("missing `--conclusion`".to_string()))?;
	let duration_seconds = matches
		.get_one::<u64>("duration-seconds")
		.copied()
		.unwrap_or(0);
	let commit = matches.get_one::<String>("commit").cloned();
	let pr = matches.get_one::<String>("pr").cloned();
	let branch = matches
		.get_one::<String>("branch")
		.cloned()
		.or_else(|| Some("main".to_string()));
	let explicit_redundant = matches.get_flag("redundant");

	let explicit_changed_files: Vec<String> = matches
		.get_many::<String>("changed-files")
		.map(|vals| {
			vals.map(|s| normalize_changed_path(s))
				.filter(|s| !s.is_empty())
				.collect()
		})
		.unwrap_or_default();

	// Look up matching prior decision in memory
	let prior_decision_record = memory
		.decisions
		.iter()
		.rev()
		.find(|d| d.job == job && (commit.is_none() || d.commit.as_deref() == commit.as_deref()))
		.or_else(|| memory.decisions.iter().rev().find(|d| d.job == job))
		.cloned();

	let changed_files = if !explicit_changed_files.is_empty() {
		explicit_changed_files
	} else if let Some(ref prior) = prior_decision_record {
		prior.changed_files.clone()
	} else {
		Vec::new()
	};

	let graph_ctx = evaluate_workspace_graph_context(root, &changed_files);
	let (graph_would_run, _, affected_packages) =
		evaluate_graph_decision_for_job(&job, &changed_files, &[], &[], &graph_ctx);

	let prior_decision = if let Some(explicit_prior) = matches.get_one::<String>("prior-decision") {
		if matches!(explicit_prior.as_str(), "y" | "true" | "run") {
			"y".to_string()
		} else {
			"n".to_string()
		}
	} else if let Some(ref prior) = prior_decision_record {
		if prior.unconditional && !graph_would_run && conclusion == "failure" {
			// A forced unconditional run on `main` caught a failure that the conditional gate would have skipped!
			"n".to_string()
		} else {
			prior.decision.clone()
		}
	} else if graph_would_run {
		"y".to_string()
	} else {
		"n".to_string()
	};

	let is_failure = conclusion == "failure";
	let (classification, reason) = match (prior_decision.as_str(), is_failure) {
		("n", true) => {
			(
				CiVerificationClassification::FalseNegative,
				format!(
					"job `{job}` was skipped (`n`) or would have been skipped, but failed on verification (`{conclusion}`); adding learned override rule"
				),
			)
		}
		("n", false) => {
			(
				CiVerificationClassification::TrueNegative,
				format!(
					"job `{job}` was safely skipped (`n`) and verified `{conclusion}` on {}",
					branch.as_deref().unwrap_or("main")
				),
			)
		}
		(_, true) => {
			(
				CiVerificationClassification::TruePositive,
				format!("job `{job}` ran (`y`) and caught a failure"),
			)
		}
		(_, false) => {
			let unaffected_and_docs =
				!changed_files.is_empty() && (graph_ctx.docs_or_ignored_only || !graph_would_run);
			if explicit_redundant || unaffected_and_docs {
				(
					CiVerificationClassification::FalsePositive,
					format!(
						"job `{job}` ran (`y`) and passed, but changed files did not affect its packages or only touched non-code/unrelated paths"
					),
				)
			} else {
				(
					CiVerificationClassification::TruePositive,
					format!(
						"job `{job}` ran (`y`) and verified affected changes with `{conclusion}`"
					),
				)
			}
		}
	};

	let learned_rule_added = if classification == CiVerificationClassification::FalseNegative {
		let patterns = derive_learned_patterns_from_files(&changed_files);
		let existing = memory
			.learned_rules
			.iter()
			.find(|rule| rule.job == job && rule.patterns == patterns)
			.cloned();
		if let Some(rule) = existing {
			Some(rule)
		} else {
			let rule = CiLearnedRule {
				id: format!("rule-{}-{}", memory.learned_rules.len() + 1, unix_millis()),
				job: job.clone(),
				patterns,
				packages: affected_packages.clone(),
				reason: format!(
					"Auto-learned from false_negative on `{job}`{}",
					commit
						.as_deref()
						.map(|c| format!(" at commit {c}"))
						.unwrap_or_default()
				),
				source_commit: commit.clone(),
				created_at: Utc::now().to_rfc3339(),
			};
			memory.learned_rules.push(rule.clone());
			Some(rule)
		}
	} else {
		None
	};

	if is_main_branch(branch.as_deref(), None) && conclusion == "success" {
		memory.consecutive_skips_on_main.insert(job.clone(), 0);
	}

	let max_duration_minutes = matches
		.get_one::<u64>("max-duration-minutes")
		.copied()
		.unwrap_or(DEFAULT_MAX_DURATION_MINUTES);
	let rust_max_duration_minutes = matches
		.get_one::<u64>("rust-max-duration-minutes")
		.copied()
		.unwrap_or(DEFAULT_RUST_MAX_DURATION_MINUTES);
	let is_rust = is_rust_job_or_context(&job, &changed_files, root);
	let threshold_minutes = if is_rust {
		rust_max_duration_minutes
	} else {
		max_duration_minutes
	};
	let duration_red_flag = duration_seconds > threshold_minutes * 60;
	let speed_suggestions = if duration_red_flag {
		build_speed_suggestions(&job, is_rust, duration_seconds, threshold_minutes)
	} else {
		Vec::new()
	};

	let verification = CiVerificationRecord {
		id: format!("ver-{}-{}", memory.verifications.len() + 1, unix_millis()),
		repo_id: memory.repo_id.clone(),
		job,
		conclusion,
		duration_seconds,
		commit,
		pr,
		branch,
		prior_decision,
		classification,
		reason,
		changed_files,
		affected_packages,
		learned_rule_added,
		duration_red_flag,
		threshold_minutes,
		speed_suggestions,
		timestamp: Utc::now().to_rfc3339(),
	};

	memory.verifications.push(verification.clone());
	save_ci_memory_store(root, &memory)?;

	let format = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	if format == "json" {
		return serde_json::to_string_pretty(&verification).map_err(|error| {
			MonochangeError::Io(format!("failed to serialize verification report: {error}"))
		});
	}

	let mut lines = vec![
		format!("job: {}", verification.job),
		format!("conclusion: {}", verification.conclusion),
		format!("prior_decision: {}", verification.prior_decision),
		format!("classification: {}", verification.classification.as_str()),
		format!("reason: {}", verification.reason),
	];
	if let Some(ref rule) = verification.learned_rule_added {
		lines.push(format!(
			"learned_rule_added: {} (patterns: {})",
			rule.id,
			rule.patterns.join(", ")
		));
	}
	if verification.duration_red_flag {
		for suggestion in &verification.speed_suggestions {
			lines.push(format!("speed_suggestion: {suggestion}"));
		}
	}
	Ok(lines.join("\n"))
}

fn run_ci_analytics_subcommand(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let repo_id_override = matches.get_one::<String>("repo-id").map(String::as_str);
	let explicit_token = matches.get_one::<String>("token").map(String::as_str);

	// Analytics ALWAYS requires valid repo authentication
	let auth = authenticate_repo_access(root, repo_id_override, explicit_token)?;
	let memory = load_ci_memory_store(root, repo_id_override, true, explicit_token)?;

	let max_duration_minutes = matches
		.get_one::<u64>("max-duration-minutes")
		.copied()
		.unwrap_or(DEFAULT_MAX_DURATION_MINUTES);
	let rust_max_duration_minutes = matches
		.get_one::<u64>("rust-max-duration-minutes")
		.copied()
		.unwrap_or(DEFAULT_RUST_MAX_DURATION_MINUTES);

	let report = compute_analytics_report(
		root,
		&auth,
		&memory,
		max_duration_minutes,
		rust_max_duration_minutes,
	);

	let format = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	render_analytics_report(&report, format)
}

fn compute_analytics_report(
	root: &Path,
	auth: &CiAuthReport,
	memory: &CiMemoryStore,
	max_duration_minutes: u64,
	rust_max_duration_minutes: u64,
) -> CiAnalyticsReport {
	let total_decisions = memory.decisions.len();
	let run_decisions = memory.decisions.iter().filter(|d| d.run).count();
	let skip_decisions = memory.decisions.iter().filter(|d| !d.run).count();
	let unconditional_decisions = memory.decisions.iter().filter(|d| d.unconditional).count();
	let skip_rate = if total_decisions > 0 {
		skip_decisions as f64 / total_decisions as f64
	} else {
		0.0
	};

	// Compute average duration per job (in minutes), defaulting to 5.0 minutes when unknown
	let mut job_durations: BTreeMap<String, Vec<u64>> = BTreeMap::new();
	for ver in &memory.verifications {
		if ver.duration_seconds > 0 {
			job_durations
				.entry(ver.job.clone())
				.or_default()
				.push(ver.duration_seconds);
		}
	}
	for dec in &memory.decisions {
		if let Some(secs) = dec.observed_duration_seconds
			&& secs > 0
		{
			job_durations.entry(dec.job.clone()).or_default().push(secs);
		}
	}

	let avg_minutes_for_job = |job_name: &str| -> f64 {
		if let Some(samples) = job_durations.get(job_name)
			&& !samples.is_empty()
		{
			let sum: u64 = samples.iter().sum();
			(sum as f64 / samples.len() as f64) / 60.0
		} else {
			5.0
		}
	};

	let estimated_vm_minutes_saved: f64 = memory
		.decisions
		.iter()
		.filter(|d| !d.run)
		.map(|d| avg_minutes_for_job(&d.job))
		.sum();

	let mut true_positives = 0_usize;
	let mut true_negatives = 0_usize;
	let mut false_positives = 0_usize;
	let mut false_negatives = 0_usize;
	let mut false_negative_incidents = Vec::new();
	let mut false_positive_incidents = Vec::new();
	let mut wasted_vm_minutes = 0.0_f64;

	for ver in &memory.verifications {
		match ver.classification {
			CiVerificationClassification::TruePositive => true_positives += 1,
			CiVerificationClassification::TrueNegative => true_negatives += 1,
			CiVerificationClassification::FalseNegative => {
				false_negatives += 1;
				false_negative_incidents.push(CiFalseNegativeIncident {
					id: ver.id.clone(),
					job: ver.job.clone(),
					commit: ver.commit.clone(),
					pr: ver.pr.clone(),
					changed_files: ver.changed_files.clone(),
					reason: ver.reason.clone(),
					learned_rule: ver.learned_rule_added.clone(),
					timestamp: ver.timestamp.clone(),
				});
			}
			CiVerificationClassification::FalsePositive => {
				false_positives += 1;
				let wasted = if ver.duration_seconds > 0 {
					ver.duration_seconds as f64 / 60.0
				} else {
					avg_minutes_for_job(&ver.job)
				};
				wasted_vm_minutes += wasted;
				let recommendation = format!(
					"Restrict job `{}` with `--job-packages` or `--job-paths`, or add ignored paths for `{}` so unrelated changes skip (`n`).",
					ver.job,
					ver.changed_files.join(", ")
				);
				false_positive_incidents.push(CiFalsePositiveIncident {
					id: ver.id.clone(),
					job: ver.job.clone(),
					commit: ver.commit.clone(),
					pr: ver.pr.clone(),
					changed_files: ver.changed_files.clone(),
					wasted_vm_minutes: wasted,
					reason: ver.reason.clone(),
					recommendation,
					timestamp: ver.timestamp.clone(),
				});
			}
		}
	}

	let precision = if true_positives + false_positives > 0 {
		true_positives as f64 / (true_positives + false_positives) as f64
	} else {
		1.0
	};
	let recall = if true_positives + false_negatives > 0 {
		true_positives as f64 / (true_positives + false_negatives) as f64
	} else {
		1.0
	};
	let false_negative_rate = if true_positives + false_negatives > 0 {
		false_negatives as f64 / (true_positives + false_negatives) as f64
	} else {
		0.0
	};

	let mut slow_jobs = Vec::new();
	for (job, samples) in &job_durations {
		let max_secs = samples.iter().copied().max().unwrap_or(0);
		let avg_secs = samples.iter().sum::<u64>() as f64 / samples.len() as f64;
		let is_rust = is_rust_job_or_context(job, &[], root);
		let threshold = if is_rust {
			rust_max_duration_minutes
		} else {
			max_duration_minutes
		};
		if max_secs > threshold * 60 {
			slow_jobs.push(CiSlowJobReport {
				job: job.clone(),
				max_duration_seconds: max_secs,
				average_duration_seconds: avg_secs,
				threshold_minutes: threshold,
				is_rust_job: is_rust,
				suggestions: build_speed_suggestions(job, is_rust, max_secs, threshold),
			});
		}
	}

	let mut tuning_recommendations = Vec::new();
	if !false_positive_incidents.is_empty() {
		for fp in &false_positive_incidents {
			if !tuning_recommendations.contains(&fp.recommendation) {
				tuning_recommendations.push(fp.recommendation.clone());
			}
		}
	}
	if !false_negative_incidents.is_empty() {
		tuning_recommendations.push(format!(
			"{} false-negative incident(s) detected; {} learned rule(s) are now active in `.monochange/ci-memory.json` to force `y` on matching paths.",
			false_negative_incidents.len(),
			memory.learned_rules.len()
		));
	}
	if !slow_jobs.is_empty() {
		tuning_recommendations.push(format!(
			"{} slow job(s) exceeded duration red-flag thresholds; apply caching and test sharding recommendations.",
			slow_jobs.len()
		));
	}

	CiAnalyticsReport {
		repo_id: auth.repo_id.clone(),
		repo_hash: auth.repo_hash.clone(),
		authenticated: true,
		total_decisions,
		run_decisions,
		skip_decisions,
		unconditional_decisions,
		skip_rate,
		estimated_vm_minutes_saved,
		wasted_vm_minutes,
		true_positives,
		true_negatives,
		false_positives,
		false_negatives,
		precision,
		recall,
		false_negative_rate,
		learned_rules_count: memory.learned_rules.len(),
		false_negative_incidents,
		false_positive_incidents,
		tuning_recommendations,
		slow_jobs,
	}
}

fn render_analytics_report(report: &CiAnalyticsReport, format: &str) -> MonochangeResult<String> {
	if format == "json" {
		return serde_json::to_string_pretty(report).map_err(|error| {
			MonochangeError::Io(format!("failed to serialize analytics report: {error}"))
		});
	}

	let mut lines = Vec::new();
	if format == "markdown" || format == "md" {
		lines.push(format!("# CI Gatekeeper Analytics (`{}`)", report.repo_id));
		lines.push(String::new());
		lines.push("## Summary Metrics".to_string());
		lines.push(format!(
			"- **Total Decisions**: {} (`y`: {}, `n`: {}, unconditional: {})",
			report.total_decisions,
			report.run_decisions,
			report.skip_decisions,
			report.unconditional_decisions
		));
		lines.push(format!("- **Skip Rate**: {:.1}%", report.skip_rate * 100.0));
		lines.push(format!(
			"- **Estimated VM-Minutes Saved**: {:.1}m",
			report.estimated_vm_minutes_saved
		));
		lines.push(format!(
			"- **Wasted VM-Minutes (False Positives)**: {:.1}m",
			report.wasted_vm_minutes
		));
		lines.push(format!(
			"- **Classification Counts**: `true_positive`: {}, `true_negative`: {}, `false_positive`: {}, `false_negative`: {}",
			report.true_positives,
			report.true_negatives,
			report.false_positives,
			report.false_negatives
		));
		lines.push(format!(
			"- **Precision**: {:.1}% | **Recall**: {:.1}% | **False-Negative Rate**: {:.1}%",
			report.precision * 100.0,
			report.recall * 100.0,
			report.false_negative_rate * 100.0
		));
		lines.push(String::new());
		lines.push("## Runs That Didn't Run (`n`) But Should Have (`false_negative`)".to_string());
		if report.false_negative_incidents.is_empty() {
			lines.push("None.".to_string());
		} else {
			for inc in &report.false_negative_incidents {
				let rule_desc = inc.learned_rule.as_ref().map_or_else(
					|| "none".to_string(),
					|r| format!("{} ({})", r.id, r.patterns.join(", ")),
				);
				lines.push(format!(
					"- **Job `{}`** (commit: `{}`, PR: `{}`): files `[{}]` — learned rule: `{}`",
					inc.job,
					inc.commit.as_deref().unwrap_or("n/a"),
					inc.pr.as_deref().unwrap_or("n/a"),
					inc.changed_files.join(", "),
					rule_desc
				));
			}
		}
		lines.push(String::new());
		lines.push("## Runs That Ran (`y`) But Shouldn't Have (`false_positive`)".to_string());
		if report.false_positive_incidents.is_empty() {
			lines.push("None.".to_string());
		} else {
			for inc in &report.false_positive_incidents {
				lines.push(format!(
					"- **Job `{}`** (commit: `{}`, wasted: {:.1}m): files `[{}]` — {}",
					inc.job,
					inc.commit.as_deref().unwrap_or("n/a"),
					inc.wasted_vm_minutes,
					inc.changed_files.join(", "),
					inc.recommendation
				));
			}
		}
		lines.push(String::new());
		lines.push("## Slow Job Red Flags".to_string());
		if report.slow_jobs.is_empty() {
			lines.push("None.".to_string());
		} else {
			for slow in &report.slow_jobs {
				lines.push(format!(
					"- **Job `{}`**: max {:.1}m (threshold: {}m, rust: {})",
					slow.job,
					slow.max_duration_seconds as f64 / 60.0,
					slow.threshold_minutes,
					slow.is_rust_job
				));
				for sug in &slow.suggestions {
					lines.push(format!("  - {sug}"));
				}
			}
		}
		return Ok(lines.join("\n"));
	}

	lines.push(format!("repo_id: {}", report.repo_id));
	lines.push(format!(
		"total_decisions: {} (y: {}, n: {}, unconditional: {})",
		report.total_decisions,
		report.run_decisions,
		report.skip_decisions,
		report.unconditional_decisions
	));
	lines.push(format!("skip_rate: {:.1}%", report.skip_rate * 100.0));
	lines.push(format!(
		"estimated_vm_minutes_saved: {:.1}",
		report.estimated_vm_minutes_saved
	));
	lines.push(format!(
		"wasted_vm_minutes: {:.1}",
		report.wasted_vm_minutes
	));
	lines.push(format!(
		"true_positive: {}, true_negative: {}, false_positive: {}, false_negative: {}",
		report.true_positives,
		report.true_negatives,
		report.false_positives,
		report.false_negatives
	));
	lines.push(format!(
		"precision: {:.3}, recall: {:.3}, false_negative_rate: {:.3}",
		report.precision, report.recall, report.false_negative_rate
	));
	lines.push("false_negatives (didn't run `n`, but should have):".to_string());
	if report.false_negative_incidents.is_empty() {
		lines.push("  none".to_string());
	} else {
		for inc in &report.false_negative_incidents {
			let rule_desc = inc.learned_rule.as_ref().map_or_else(
				|| "none".to_string(),
				|r| format!("{} [{}]", r.id, r.patterns.join(", ")),
			);
			lines.push(format!(
				"  - job={} commit={} files=[{}] learned_rule={}",
				inc.job,
				inc.commit.as_deref().unwrap_or("n/a"),
				inc.changed_files.join(", "),
				rule_desc
			));
		}
	}
	lines.push("false_positives (ran `y`, but shouldn't have):".to_string());
	if report.false_positive_incidents.is_empty() {
		lines.push("  none".to_string());
	} else {
		for inc in &report.false_positive_incidents {
			lines.push(format!(
				"  - job={} commit={} wasted_minutes={:.1} files=[{}] recommendation={}",
				inc.job,
				inc.commit.as_deref().unwrap_or("n/a"),
				inc.wasted_vm_minutes,
				inc.changed_files.join(", "),
				inc.recommendation
			));
		}
	}
	lines.push("slow_jobs:".to_string());
	if report.slow_jobs.is_empty() {
		lines.push("  none".to_string());
	} else {
		for slow in &report.slow_jobs {
			lines.push(format!(
				"  - job={} max_seconds={} threshold_minutes={}",
				slow.job, slow.max_duration_seconds, slow.threshold_minutes
			));
			for sug in &slow.suggestions {
				lines.push(format!("    suggestion: {sug}"));
			}
		}
	}
	Ok(lines.join("\n"))
}

fn run_ci_history_subcommand(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let repo_id_override = matches.get_one::<String>("repo-id").map(String::as_str);
	let explicit_token = matches.get_one::<String>("token").map(String::as_str);

	// History ALWAYS requires valid repo authentication
	let auth = authenticate_repo_access(root, repo_id_override, explicit_token)?;
	let memory = load_ci_memory_store(root, repo_id_override, true, explicit_token)?;

	let job_filter = matches.get_one::<String>("job").map(String::as_str);
	let limit = matches.get_one::<usize>("limit").copied();

	let mut decisions: Vec<CiDecisionRecord> = memory
		.decisions
		.into_iter()
		.filter(|d| job_filter.is_none_or(|j| d.job == j))
		.collect();
	let mut verifications: Vec<CiVerificationRecord> = memory
		.verifications
		.into_iter()
		.filter(|v| job_filter.is_none_or(|j| v.job == j))
		.collect();

	if let Some(max_items) = limit {
		if decisions.len() > max_items {
			decisions = decisions.split_off(decisions.len() - max_items);
		}
		if verifications.len() > max_items {
			verifications = verifications.split_off(verifications.len() - max_items);
		}
	}

	let report = CiHistoryReport {
		repo_id: auth.repo_id,
		repo_hash: auth.repo_hash,
		authenticated: true,
		learned_rules: memory.learned_rules,
		decisions,
		verifications,
	};

	let format = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	if format == "json" {
		return serde_json::to_string_pretty(&report).map_err(|error| {
			MonochangeError::Io(format!("failed to serialize history report: {error}"))
		});
	}

	let mut lines = vec![
		format!("repo_id: {}", report.repo_id),
		format!("decisions: {}", report.decisions.len()),
		format!("verifications: {}", report.verifications.len()),
		format!("learned_rules: {}", report.learned_rules.len()),
	];
	for d in &report.decisions {
		lines.push(format!(
			"decision [{}]: job={} decision={} unconditional={} provider={} reason={}",
			d.id, d.job, d.decision, d.unconditional, d.provider, d.reason
		));
	}
	for v in &report.verifications {
		lines.push(format!(
			"verification [{}]: job={} conclusion={} classification={} prior={}",
			v.id,
			v.job,
			v.conclusion,
			v.classification.as_str(),
			v.prior_decision
		));
	}
	Ok(lines.join("\n"))
}

fn unix_millis() -> u128 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |d| d.as_millis())
}

#[cfg(test)]
#[path = "__tests__/ci_tests.rs"]
mod tests;
