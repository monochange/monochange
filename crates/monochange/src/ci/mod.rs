//! Conditional CI gatekeeper commands.
//!
//! `monochange ci` answers two questions in a loop: should a CI job run for this
//! change, and did the answer turn out to be right? The command tree combines a
//! deterministic workspace graph, an optional external model provider, a
//! repo-scoped HMAC auth layer, and a local memory store that learns from
//! false negatives.
//!
//! The implementation is split by responsibility rather than kept in one file so
//! each piece stays small enough to review on its own: argument plumbing lives
//! in `args`, token handling in `auth`, graph resolution in `graph`, provider
//! calls in `provider`, and one module per subcommand.

mod analytics;
mod args;
mod auth;
mod cli;
mod decide;
mod graph;

mod history;
mod provider;
mod render;
mod types;
mod verify;

use std::path::Path;

use analytics::run_ci_analytics_subcommand;
use auth::authenticate_repo_access;
pub use auth::compute_repo_hash;
pub use auth::derive_canonical_repo_id;
pub use auth::issue_repo_token;
pub use auth::normalize_repo_identity;
pub use auth::verify_repo_token;
use clap::ArgMatches;
pub(crate) use cli::build_ci_subcommand;
use decide::run_ci_decide_subcommand;
pub use graph::build_speed_suggestions;
use history::run_ci_history_subcommand;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
pub use types::CiAnalyticsReport;
pub use types::CiAuthFile;
pub use types::CiAuthReport;
pub use types::CiDecideReport;
pub use types::CiDecisionProvider;
pub use types::CiDecisionRecord;
pub use types::CiFalseNegativeIncident;
pub use types::CiFalsePositiveIncident;
pub use types::CiHistoryReport;
pub use types::CiLearnedRule;
pub use types::CiMemoryStore;
pub use types::CiSlowJobReport;
pub use types::CiVerificationClassification;
pub use types::CiVerificationRecord;
use verify::run_ci_verify_subcommand;

/// Relative path of the repo-scoped CI auth file inside the workspace root.
pub const CI_AUTH_RELATIVE_PATH: &str = ".monochange/ci-auth.json";
/// Relative path of the CI decision and verification memory store.
pub const CI_MEMORY_RELATIVE_PATH: &str = ".monochange/ci-memory.json";
/// Directory that holds the repo-scoped CI state files.
pub(super) const CI_DIRECTORY_RELATIVE_PATH: &str = ".monochange";
/// Default chat completions endpoint for the `opencode-go` provider.
pub const DEFAULT_OPENCODE_GO_API_URL: &str = "https://api.opencode.ai/v1/chat/completions";
/// Default model identifier for the `opencode-go` and `openai-compatible` providers.
pub const DEFAULT_OPENCODE_GO_MODEL: &str = "deepseek-v4.1";
/// Default chat completions endpoint for the `openrouter` provider.
pub const DEFAULT_OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
/// Default model identifier for the `openrouter` provider.
pub const DEFAULT_OPENROUTER_MODEL: &str = "deepseek/deepseek-chat";
/// Default Workers AI model identifier for the `cloudflare` provider.
pub const DEFAULT_CLOUDFLARE_MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
/// Consecutive skips allowed on `main` before a forced verification run.
pub const DEFAULT_MAX_SKIPS_ON_MAIN: u32 = 5;
/// Default duration red-flag threshold for general CI jobs.
pub const DEFAULT_MAX_DURATION_MINUTES: u64 = 10;
/// Default duration red-flag threshold for Rust CI jobs.
pub const DEFAULT_RUST_MAX_DURATION_MINUTES: u64 = 20;

/// Events that always run every job regardless of the gate decision.
const DEFAULT_UNCONDITIONAL_EVENTS: &[&str] = &["schedule", "workflow_dispatch"];
/// Branch and ref globs that always run every job.
const DEFAULT_UNCONDITIONAL_BRANCHES: &[&str] = &[
	"release/*",
	"monochange/release/*",
	"refs/tags/v*",
	"refs/heads/release/*",
	"refs/heads/monochange/release/*",
];
/// Changed paths that always run every job because they affect the toolchain.
const DEFAULT_UNCONDITIONAL_PATHS: &[&str] = &[
	"Cargo.lock",
	"rust-toolchain.toml",
	"rust-toolchain",
	".github/workflows/*",
	".github/workflows/**",
];
/// Labels and commit-message markers that force a full CI run.
const UNCONDITIONAL_MARKERS: &[&str] = &[
	"[ci:full]",
	"[ci:unconditional]",
	"ci:full",
	"ci:unconditional",
];

/// Dispatch the selected `monochange ci` subcommand.
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

/// Dispatch the selected `monochange ci auth` subcommand.
fn run_ci_auth_subcommand(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	match matches.subcommand() {
		Some(("init", sub_matches)) => run_auth_init(root, sub_matches),
		Some(("verify", sub_matches)) => run_auth_verify(root, sub_matches),
		Some(("token", sub_matches)) => run_auth_token(root, sub_matches),
		_ => {
			Err(MonochangeError::Config(
				"missing `monochange ci auth` subcommand".to_string(),
			))
		}
	}
}

/// Generate a fresh secret and persist the repo-scoped token.
///
/// Refuses to overwrite an existing auth file unless `--force` is passed, so a
/// stray CI invocation cannot rotate the secret under a running pipeline.
fn run_auth_init(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let force = matches.get_flag("force");
	let auth_path = auth::ci_auth_file_path(root);

	if auth_path.exists() && !force {
		return Err(MonochangeError::Config(format!(
			"{} already exists; pass `--force` to regenerate the repository secret and token",
			auth_path.display()
		)));
	}

	let repo_id = derive_canonical_repo_id(root, args::repo_id_override(matches));
	let repo_hash = compute_repo_hash(&repo_id);
	let secret = auth::configured_or_generated_secret()?;
	let token = issue_repo_token(&repo_id, &secret);
	let auth_file = CiAuthFile {
		schema_version: auth::AUTH_SCHEMA_VERSION,
		repo_id: repo_id.clone(),
		repo_hash: repo_hash.clone(),
		secret,
		token: token.clone(),
		created_at: chrono::Utc::now().to_rfc3339(),
	};
	let saved_path = auth::save_ci_auth_file(root, &auth_file)?;
	let report = CiAuthReport {
		action: "init".to_string(),
		authenticated: true,
		repo_id,
		repo_hash,
		token,
		auth_file: saved_path.display().to_string(),
		source: CI_AUTH_RELATIVE_PATH.to_string(),
	};

	render::render_auth_report(&report, args::format_arg(matches), false)
}

/// Verify the caller's token against the current repository.
fn run_auth_verify(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let report = authenticate_repo_access(
		root,
		args::repo_id_override(matches),
		args::explicit_token(matches),
	)?;

	render::render_auth_report(&report, args::format_arg(matches), false)
}

/// Print the active token so it can be copied into a CI secret.
fn run_auth_token(root: &Path, matches: &ArgMatches) -> MonochangeResult<String> {
	let mut report = authenticate_repo_access(root, args::repo_id_override(matches), None)?;
	report.action = "token".to_string();

	render::render_auth_report(&report, args::format_arg(matches), true)
}

#[cfg(test)]
#[path = "../__tests__/ci_tests.rs"]
mod tests;
