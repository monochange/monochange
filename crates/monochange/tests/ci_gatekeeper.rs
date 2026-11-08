#![allow(clippy::disallowed_methods)]
//! Integration tests for `monochange ci` (`auth`, `decide`, `verify`, `analytics`, `history`).
use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use httpmock::Method::POST;
use httpmock::MockServer;
use monochange::ci::CI_AUTH_RELATIVE_PATH;
use monochange::ci::CI_MEMORY_RELATIVE_PATH;
use monochange::ci::CiAnalyticsReport;
use monochange::ci::CiAuthReport;
use monochange::ci::CiDecideReport;
use monochange::ci::CiHistoryReport;
use monochange::ci::CiVerificationClassification;
use monochange::ci::CiVerificationRecord;
use monochange::ci::issue_repo_token;
use serde_json::json;
use tempfile::tempdir;

/// Successful HTTP status used by the mocked providers.
const HTTP_OK: u16 = 200;
/// Arguments that initialise auth and print JSON.
const AUTH_INIT_JSON_ARGS: &[&str] = &["ci", "auth", "init", "--format", "json"];
/// Arguments that print the active auth token.
const AUTH_TOKEN_ARGS: &[&str] = &["ci", "auth", "token"];
/// Arguments that render analytics as JSON.
const ANALYTICS_JSON_ARGS: &[&str] = &["ci", "analytics", "--format", "json"];
/// Arguments that render history as JSON.
const HISTORY_JSON_ARGS: &[&str] = &["ci", "history", "--format", "json"];

/// Path of the compiled `monochange` binary.
fn mc_bin() -> std::path::PathBuf {
	insta_cmd::get_cargo_bin("monochange")
}

/// Run the CLI in `root` with a clean CI environment plus explicit overrides.
fn run_cli(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
	let mut cmd = ProcessCommand::new(mc_bin());

	cmd.current_dir(root);
	cmd.env("NO_COLOR", "1");
	cmd.env_remove("MONOCHANGE_CI_TOKEN");
	cmd.env_remove("MONOCHANGE_CI_SECRET");
	cmd.env_remove("MONOCHANGE_CI_REPO_ID");
	for name in [
		"JEV_API_URL",
		"JEV_API_KEY",
		"OPENAI_API_URL",
		"OPENAI_API_KEY",
		"OPENROUTER_API_URL",
		"OPENROUTER_API_KEY",
		"CLOUDFLARE_API_URL",
		"CLOUDFLARE_API_TOKEN",
		"CLOUDFLARE_ACCOUNT_ID",
		"OPENCODE_GO_API_URL",
		"OPENCODE_GO_API_KEY",
	] {
		cmd.env_remove(name);
	}

	for (key, value) in envs {
		cmd.env(key, value);
	}

	cmd.args(args);

	cmd.output().unwrap()
}

/// Create a git repository with an `origin` remote.
fn init_git_repo(root: &Path, remote_url: &str) {
	ProcessCommand::new("git")
		.current_dir(root)
		.args(["init"])
		.output()
		.unwrap();
	ProcessCommand::new("git")
		.current_dir(root)
		.args(["remote", "add", "origin", remote_url])
		.output()
		.unwrap();
}

/// Arguments that verify one auth token.
fn auth_verify_args(token: &str) -> [&str; 5] {
	["ci", "auth", "verify", "--token", token]
}

/// Arguments that request analytics with an explicit token.
fn analytics_token_args(token: &str) -> [&str; 4] {
	["ci", "analytics", "--token", token]
}

/// Run `ci auth init --format json` and return the parsed report.
fn init_repo_auth(repo: &Path) -> CiAuthReport {
	let output = run_cli(repo, AUTH_INIT_JSON_ARGS, &[]);

	assert!(
		output.status.success(),
		"stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	serde_json::from_slice(&output.stdout).unwrap()
}

/// Assert that `ci auth token` prints exactly the stored token.
fn assert_token_output(repo: &Path, auth: &CiAuthReport) {
	let output = run_cli(repo, AUTH_TOKEN_ARGS, &[]);

	assert!(output.status.success());
	assert_eq!(
		String::from_utf8_lossy(&output.stdout).trim(),
		auth.token.as_str()
	);
}

/// Assert that a token minted for another repository is rejected.
fn assert_cross_repo_token_rejected(repo: &Path, token: &str) {
	let output = run_cli(repo, &auth_verify_args(token), &[]);

	assert!(!output.status.success());

	let stderr = String::from_utf8_lossy(&output.stderr);

	assert!(stderr.contains("cross-repo access denied") || stderr.contains("repository mismatch"));
}

/// Assert that `ci decide --require-auth` rejects a foreign token.
fn assert_decide_token_rejected(repo: &Path, token: &str) {
	let output = run_cli(
		repo,
		&[
			"ci",
			"decide",
			"--require-auth",
			"--token",
			token,
			"--job",
			"rust-test",
		],
		&[],
	);

	assert!(!output.status.success());
}

/// Write the gated workspace and `[ci]` configuration used by end-to-end runs.
fn write_gated_workspace(repo: &Path) {
	for directory in ["crates/core/src", "crates/app/src"] {
		fs::create_dir_all(repo.join(directory)).unwrap();
	}

	fs::write(
		repo.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/core\", \"crates/app\"]\nresolver = \"2\"\n",
	)
	.unwrap();
	fs::write(
		repo.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap();
	fs::write(repo.join("crates/core/src/lib.rs"), "pub fn core() {}\n").unwrap();
	fs::write(
		repo.join("crates/app/Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[dependencies]\ncore = { path = \"../core\" }\n",
	)
	.unwrap();
	fs::write(repo.join("crates/app/src/lib.rs"), "pub fn app() {}\n").unwrap();
	fs::write(
		repo.join("monochange.toml"),
		"[ci.jobs.rust-test]\npackages = [\"app\"]\n",
	)
	.unwrap();
}

/// Run a graph-backed decision for one job over the supplied files.
fn decide_graph(repo: &Path, job: &str, changed_files: &str, commit: &str) -> CiDecideReport {
	let output = run_cli(
		repo,
		&[
			"ci",
			"decide",
			"--job",
			job,
			"--changed-files",
			changed_files,
			"--commit",
			commit,
			"--format",
			"json",
		],
		&[],
	);

	assert!(output.status.success());

	serde_json::from_slice(&output.stdout).unwrap()
}

/// Record a failed verification for `rust-test` against commit `c1`.
fn verify_rust_test_failure(repo: &Path) -> CiVerificationRecord {
	let output = run_cli(
		repo,
		&[
			"ci",
			"verify",
			"--job",
			"rust-test",
			"--conclusion",
			"failure",
			"--duration-seconds",
			"1500",
			"--commit",
			"c1",
			"--changed-files",
			"README.md",
			"--format",
			"json",
		],
		&[],
	);

	assert!(output.status.success());

	serde_json::from_slice(&output.stdout).unwrap()
}

/// Assert analytics and history output for the authenticated repo.
fn assert_analytics_and_history(repo: &Path, forged_token: &str) {
	let analytics_bad = run_cli(repo, &analytics_token_args(forged_token), &[]);

	assert!(!analytics_bad.status.success());

	let analytics_ok = run_cli(repo, ANALYTICS_JSON_ARGS, &[]);

	assert!(analytics_ok.status.success());

	let analytics: CiAnalyticsReport = serde_json::from_slice(&analytics_ok.stdout).unwrap();

	assert!(analytics.authenticated);
	assert_eq!(analytics.total_decisions, 2);
	assert_eq!(analytics.false_negatives, 1);
	assert_eq!(analytics.learned_rules_count, 1);
	assert_eq!(analytics.slow_jobs.len(), 1);

	let history_ok = run_cli(repo, HISTORY_JSON_ARGS, &[]);

	assert!(history_ok.status.success());

	let history: CiHistoryReport = serde_json::from_slice(&history_ok.stdout).unwrap();

	assert_eq!(history.decisions.len(), 2);
	assert_eq!(history.verifications.len(), 1);
}

#[test]
fn cli_ci_end_to_end_auth_decide_verify_analytics_and_cross_repo_protection() {
	let repo_a = tempdir().unwrap();
	let repo_b = tempdir().unwrap();

	init_git_repo(repo_a.path(), "https://github.com/monochange/repo-a.git");
	init_git_repo(repo_b.path(), "https://github.com/monochange/repo-b.git");

	write_gated_workspace(repo_a.path());

	let auth_a = init_repo_auth(repo_a.path());

	assert_eq!(auth_a.repo_id, "github.com/monochange/repo-a");
	assert!(repo_a.path().join(CI_AUTH_RELATIVE_PATH).is_file());
	assert_token_output(repo_a.path(), &auth_a);

	let forged_b_token = issue_repo_token("https://github.com/monochange/repo-b.git", "secret-b");

	assert_cross_repo_token_rejected(repo_a.path(), &forged_b_token);
	assert_decide_token_rejected(repo_a.path(), &forged_b_token);

	// `README.md` matches `[ci] ignore_paths`, so the gated job skips.
	let decide_skip = decide_graph(repo_a.path(), "rust-test", "README.md", "c1");

	assert_eq!(decide_skip.decision, "n");
	assert!(!decide_skip.run);
	assert!(repo_a.path().join(CI_MEMORY_RELATIVE_PATH).is_file());

	let verification = verify_rust_test_failure(repo_a.path());

	assert_eq!(
		verification.classification,
		CiVerificationClassification::FalseNegative
	);
	assert!(verification.learned_rule_added.is_some());
	assert!(verification.duration_red_flag);

	let decide_learned = decide_graph(repo_a.path(), "rust-test", "README.md", "c2");

	assert_eq!(decide_learned.decision, "y");
	assert!(decide_learned.unconditional);

	assert_analytics_and_history(repo_a.path(), &forged_b_token);
}

/// Register an `OpenRouter` mock that approves the job.
fn mock_openrouter(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(POST).path("/openrouter");
		then.status(HTTP_OK).json_body(json!({
			"choices": [{
				"message": {
					"content": "{\"cargo-test\":{\"run\":true,\"reason\":\"openrouter matched crate change\"}}"
				}
			}]
		}));
	})
}

/// Register a Cloudflare mock that rejects the job.
fn mock_cloudflare(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(POST)
			.path("/client/v4/accounts/acc123/ai/v1/chat/completions");
		then.status(HTTP_OK).json_body(json!({
			"web-lint": {"run": false, "reason": "cloudflare skipped unaffected job"}
		}));
	})
}

/// Run an `OpenRouter` decision and return what was written to `$GITHUB_OUTPUT`.
fn run_openrouter_decide(repo: &Path, server: &MockServer) -> String {
	let github_output = repo.join("github_output.txt");
	let api_url = server.url("/openrouter");
	let output = run_cli(
		repo,
		&[
			"ci",
			"decide",
			"--provider",
			"openrouter",
			"--api-url",
			&api_url,
			"--api-key",
			"or-secret",
			"--job",
			"cargo-test",
			"--changed-files",
			"crates/core/src/lib.rs",
			"--format",
			"github-outputs",
		],
		&[("GITHUB_OUTPUT", github_output.to_str().unwrap())],
	);

	assert!(output.status.success());

	fs::read_to_string(github_output).unwrap()
}

/// Run a Cloudflare decision and return the parsed report.
fn run_cloudflare_decide(repo: &Path, server: &MockServer) -> CiDecideReport {
	let api_url = server.url("/client/v4/accounts/acc123/ai/v1/chat/completions");
	let output = run_cli(
		repo,
		&[
			"ci",
			"decide",
			"--provider",
			"cloudflare",
			"--api-url",
			&api_url,
			"--api-key",
			"cf-secret",
			"--account-id",
			"acc123",
			"--job",
			"web-lint",
			"--changed-files",
			"crates/core/src/lib.rs",
			"--format",
			"json",
		],
		&[],
	);

	assert!(output.status.success());

	serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_ci_decide_providers_openrouter_cloudflare_and_github_outputs_file() {
	let server = MockServer::start();
	let openrouter_mock = mock_openrouter(&server);
	let cloudflare_mock = mock_cloudflare(&server);
	let repo = tempdir().unwrap();

	init_git_repo(repo.path(), "https://github.com/monochange/providers.git");
	write_gated_workspace(repo.path());
	fs::write(
		repo.path().join("monochange.toml"),
		"[ci]\nai_can_skip = true\n\n[ci.jobs.cargo-test]\npackages = [\"app\"]\n\n[ci.jobs.web-lint]\npackages = [\"core\"]\n",
	)
	.unwrap();

	let written = run_openrouter_decide(repo.path(), &server);

	openrouter_mock.assert();
	assert!(written.contains("decisions={\"cargo-test\":true}"));
	assert!(written.contains("affected-packages=[\"core\"]"));
	assert!(written.contains("run-all=true"));

	let report = run_cloudflare_decide(repo.path(), &server);

	cloudflare_mock.assert();
	assert_eq!(report.decision, "n");
	assert_eq!(report.provider, "cloudflare");
}

/// Register a JEV webhook mock that approves the job.
fn mock_jev_env(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(POST).path("/jev");
		then.status(HTTP_OK).json_body(json!({
			"env-jev": {"run": true, "reason": "jev env approved"}
		}));
	})
}

/// Write a workspace whose root Cargo manifest cannot be parsed by cargo.
fn break_root_manifest(repo: &Path) {
	fs::write(repo.join("Cargo.toml"), "this is [not valid toml\n").unwrap();
}

#[test]
fn cli_ci_decide_on_error_run_all_writes_outputs_file_and_exits_zero() {
	let repo = tempdir().unwrap();

	init_git_repo(repo.path(), "https://github.com/monochange/broken-gate.git");
	write_gated_workspace(repo.path());
	break_root_manifest(repo.path());

	let github_output = repo.path().join("github_output.txt");
	let output = run_cli(
		repo.path(),
		&[
			"ci",
			"decide",
			"--on-error",
			"run-all",
			"--changed-files",
			"crates/app/src/lib.rs",
			"--format",
			"github-outputs",
		],
		&[("GITHUB_OUTPUT", github_output.to_str().unwrap())],
	);

	assert!(
		output.status.success(),
		"run-all must exit 0, stderr: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let block = String::from("decisions={\"rust-test\":true}\naffected-packages=[]\nrun-all=true");

	assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), block);
	assert_eq!(
		fs::read_to_string(&github_output).unwrap(),
		format!("{block}\n")
	);

	let stderr = String::from_utf8_lossy(&output.stderr);

	assert!(
		stderr.contains("::error title=monochange ci decide::"),
		"missing annotation: {stderr}"
	);
	assert!(
		stderr.contains("cargo metadata failed"),
		"missing cargo stderr: {stderr}"
	);
}

#[test]
fn cli_ci_decide_fails_loudly_by_default_when_the_graph_cannot_be_built() {
	let repo = tempdir().unwrap();

	init_git_repo(
		repo.path(),
		"https://github.com/monochange/broken-gate-default.git",
	);
	write_gated_workspace(repo.path());
	break_root_manifest(repo.path());

	let output = run_cli(
		repo.path(),
		&[
			"ci",
			"decide",
			"--job",
			"rust-test",
			"--changed-files",
			"crates/app/src/lib.rs",
		],
		&[],
	);

	assert!(
		!output.status.success(),
		"default `--on-error fail` must exit non-zero"
	);

	let stderr = String::from_utf8_lossy(&output.stderr);

	assert!(stderr.contains("cargo metadata failed"), "stderr: {stderr}");
}

#[test]
fn cli_ci_env_fallbacks_cover_repo_identity_secret_token_and_jev_endpoint() {
	let repo = tempdir().unwrap();

	init_git_repo(repo.path(), "https://github.com/monochange/env-repo.git");

	let init_output = run_cli(
		repo.path(),
		AUTH_INIT_JSON_ARGS,
		&[("MONOCHANGE_CI_SECRET", "fixed-secret")],
	);

	assert!(init_output.status.success());

	let auth: CiAuthReport = serde_json::from_slice(&init_output.stdout).unwrap();

	assert_eq!(auth.repo_id, "github.com/monochange/env-repo");

	let secret_repo = tempdir().unwrap();

	init_git_repo(
		secret_repo.path(),
		"https://github.com/monochange/secret-repo.git",
	);

	let env_secret_output = run_cli(
		secret_repo.path(),
		&["ci", "auth", "verify", "--format", "json"],
		&[("MONOCHANGE_CI_SECRET", "fixed-secret")],
	);

	assert!(
		env_secret_output.status.success(),
		"stderr: {}",
		String::from_utf8_lossy(&env_secret_output.stderr)
	);

	let env_secret: CiAuthReport = serde_json::from_slice(&env_secret_output.stdout).unwrap();

	assert_eq!(env_secret.source, "MONOCHANGE_CI_SECRET");
	assert_eq!(env_secret.repo_id, "github.com/monochange/secret-repo");

	let token_output = run_cli(
		repo.path(),
		&["ci", "auth", "verify", "--format", "json"],
		&[("MONOCHANGE_CI_TOKEN", auth.token.as_str())],
	);

	assert!(token_output.status.success());

	let token_report: CiAuthReport = serde_json::from_slice(&token_output.stdout).unwrap();

	assert_eq!(token_report.source, "MONOCHANGE_CI_TOKEN");

	let override_output = run_cli(
		repo.path(),
		&["ci", "auth", "init", "--force", "--format", "json"],
		&[(
			"MONOCHANGE_CI_REPO_ID",
			"https://github.com/monochange/Overridden.git",
		)],
	);

	assert!(override_output.status.success());

	let overridden: CiAuthReport = serde_json::from_slice(&override_output.stdout).unwrap();

	assert_eq!(overridden.repo_id, "github.com/monochange/overridden");

	let blank_output = run_cli(
		repo.path(),
		&["ci", "auth", "init", "--force", "--format", "json"],
		&[("MONOCHANGE_CI_REPO_ID", "   ")],
	);

	assert!(blank_output.status.success());

	let blank: CiAuthReport = serde_json::from_slice(&blank_output.stdout).unwrap();

	assert_eq!(blank.repo_id, "github.com/monochange/env-repo");

	let server = MockServer::start();
	let jev_mock = mock_jev_env(&server);
	let jev_url = server.url("/jev");
	let decide_output = run_cli(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"jev",
			"--job",
			"env-jev",
			"--changed-files",
			"src/lib.rs",
			"--format",
			"json",
		],
		&[("JEV_API_URL", jev_url.as_str())],
	);

	assert!(
		decide_output.status.success(),
		"stderr: {}",
		String::from_utf8_lossy(&decide_output.stderr)
	);

	let report: CiDecideReport = serde_json::from_slice(&decide_output.stdout).unwrap();

	jev_mock.assert();
	assert_eq!(report.provider, "jev");
	assert!(report.run);
}
