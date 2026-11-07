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

fn mc_bin() -> std::path::PathBuf {
	insta_cmd::get_cargo_bin("monochange")
}

fn run_cli(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> std::process::Output {
	let mut cmd = ProcessCommand::new(mc_bin());
	cmd.current_dir(root);
	cmd.env("NO_COLOR", "1");
	cmd.env_remove("MONOCHANGE_CI_TOKEN");
	cmd.env_remove("MONOCHANGE_CI_SECRET");
	cmd.env_remove("MONOCHANGE_CI_REPO_ID");
	for (k, v) in envs {
		cmd.env(k, v);
	}
	cmd.args(args);
	cmd.output().unwrap()
}

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

#[test]
fn cli_ci_end_to_end_auth_decide_verify_analytics_and_cross_repo_protection() {
	let repo_a = tempdir().unwrap();
	let repo_b = tempdir().unwrap();
	init_git_repo(repo_a.path(), "https://github.com/monochange/repo-a.git");
	init_git_repo(repo_b.path(), "https://github.com/monochange/repo-b.git");

	// 1. Initialize repo-scoped auth in repo_a
	let init_out = run_cli(
		repo_a.path(),
		&["ci", "auth", "init", "--format", "json"],
		&[],
	);
	assert!(
		init_out.status.success(),
		"stderr: {}",
		String::from_utf8_lossy(&init_out.stderr)
	);
	let auth_a: CiAuthReport = serde_json::from_slice(&init_out.stdout).unwrap();
	assert_eq!(auth_a.repo_id, "github.com/monochange/repo-a");
	assert!(repo_a.path().join(CI_AUTH_RELATIVE_PATH).is_file());

	// 2. Verify token subcommand prints the exact token
	let token_out = run_cli(repo_a.path(), &["ci", "auth", "token"], &[]);
	assert!(token_out.status.success());
	assert_eq!(
		String::from_utf8_lossy(&token_out.stdout).trim(),
		auth_a.token
	);

	// 3. Cross-repo token must be rejected when accessing repo_a with repo_b's token
	let forged_b_token = issue_repo_token("https://github.com/monochange/repo-b.git", "secret-b");
	let bad_verify = run_cli(
		repo_a.path(),
		&["ci", "auth", "verify", "--token", &forged_b_token],
		&[],
	);
	assert!(!bad_verify.status.success());
	let stderr = String::from_utf8_lossy(&bad_verify.stderr);
	assert!(stderr.contains("cross-repo access denied") || stderr.contains("repository mismatch"));

	// 4. `decide` with `--require-auth` and a cross-repo token must be rejected
	let bad_decide = run_cli(
		repo_a.path(),
		&[
			"ci",
			"decide",
			"--require-auth",
			"--token",
			&forged_b_token,
			"--job",
			"rust-test",
		],
		&[],
	);
	assert!(!bad_decide.status.success());

	// 5. `decide` on docs-only change skips (`n`)
	let decide_skip = run_cli(
		repo_a.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-test",
			"--changed-files",
			"README.md",
			"--commit",
			"c1",
			"--format",
			"json",
		],
		&[],
	);
	assert!(decide_skip.status.success());
	let decide_rep: CiDecideReport = serde_json::from_slice(&decide_skip.stdout).unwrap();
	assert_eq!(decide_rep.decision, "n");
	assert!(!decide_rep.run);
	assert!(repo_a.path().join(CI_MEMORY_RELATIVE_PATH).is_file());

	// 6. `verify` records a false_negative when `rust-test` fails on main for commit `c1`
	let verify_fn = run_cli(
		repo_a.path(),
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
	assert!(verify_fn.status.success());
	let ver_rep: CiVerificationRecord = serde_json::from_slice(&verify_fn.stdout).unwrap();
	assert_eq!(
		ver_rep.classification,
		CiVerificationClassification::FalseNegative
	);
	assert!(ver_rep.learned_rule_added.is_some());
	assert!(ver_rep.duration_red_flag);

	// 7. Next `decide` touching `README.md` is now forced to `y` (`unconditional: true`) due to learned rule
	let decide_learned = run_cli(
		repo_a.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-test",
			"--changed-files",
			"README.md",
			"--commit",
			"c2",
			"--format",
			"json",
		],
		&[],
	);
	assert!(decide_learned.status.success());
	let decide_learned_rep: CiDecideReport =
		serde_json::from_slice(&decide_learned.stdout).unwrap();
	assert_eq!(decide_learned_rep.decision, "y");
	assert!(decide_learned_rep.unconditional);

	// 8. `analytics` and `history` succeed for authenticated local repo and reject wrong token
	let analytics_bad = run_cli(
		repo_a.path(),
		&["ci", "analytics", "--token", &forged_b_token],
		&[],
	);
	assert!(!analytics_bad.status.success());

	let analytics_ok = run_cli(repo_a.path(), &["ci", "analytics", "--format", "json"], &[]);
	assert!(analytics_ok.status.success());
	let analytics_rep: CiAnalyticsReport = serde_json::from_slice(&analytics_ok.stdout).unwrap();
	assert!(analytics_rep.authenticated);
	assert_eq!(analytics_rep.total_decisions, 2);
	assert_eq!(analytics_rep.false_negatives, 1);
	assert_eq!(analytics_rep.learned_rules_count, 1);
	assert_eq!(analytics_rep.slow_jobs.len(), 1);

	let history_ok = run_cli(repo_a.path(), &["ci", "history", "--format", "json"], &[]);
	assert!(history_ok.status.success());
	let history_rep: CiHistoryReport = serde_json::from_slice(&history_ok.stdout).unwrap();
	assert_eq!(history_rep.decisions.len(), 2);
	assert_eq!(history_rep.verifications.len(), 1);
}

#[test]
fn cli_ci_decide_providers_openrouter_cloudflare_and_github_outputs_file() {
	let server = MockServer::start();
	let openrouter_mock = server.mock(|when, then| {
		when.method(POST).path("/openrouter");
		then.status(200).json_body(json!({
			"choices": [{
				"message": {
					"content": "{\"decision\":\"y\",\"reason\":\"openrouter matched crate change\"}"
				}
			}]
		}));
	});
	let cf_mock = server.mock(|when, then| {
		when.method(POST)
			.path("/client/v4/accounts/acc123/ai/v1/chat/completions");
		then.status(200).json_body(json!({
			"decision": "n",
			"reason": "cloudflare skipped unaffected job"
		}));
	});

	let repo = tempdir().unwrap();
	init_git_repo(repo.path(), "https://github.com/monochange/providers.git");
	let gh_output = repo.path().join("github_output.txt");

	let out_or = run_cli(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"openrouter",
			"--api-url",
			&server.url("/openrouter"),
			"--api-key",
			"or-secret",
			"--job",
			"cargo-test",
			"--changed-files",
			"crates/core/src/lib.rs",
			"--format",
			"github-outputs",
		],
		&[("GITHUB_OUTPUT", gh_output.to_str().unwrap())],
	);
	assert!(out_or.status.success());
	openrouter_mock.assert();
	let written = fs::read_to_string(&gh_output).unwrap();
	assert!(written.contains("decision=y"));
	assert!(written.contains("run=true"));

	let out_cf = run_cli(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"cloudflare",
			"--api-url",
			&server.url("/client/v4/accounts/acc123/ai/v1/chat/completions"),
			"--api-key",
			"cf-secret",
			"--account-id",
			"acc123",
			"--job",
			"web-lint",
			"--changed-files",
			"README.md",
			"--format",
			"json",
		],
		&[],
	);
	assert!(out_cf.status.success());
	cf_mock.assert();
	let rep_cf: CiDecideReport = serde_json::from_slice(&out_cf.stdout).unwrap();
	assert_eq!(rep_cf.decision, "n");
	assert_eq!(rep_cf.provider, "cloudflare");
}
