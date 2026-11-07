#![allow(clippy::disallowed_methods)]

use std::fs;
use std::path::Path;
use std::process::Command as ProcessCommand;

use httpmock::Method::POST;
use httpmock::MockServer;
use serde_json::json;
use tempfile::tempdir;

use super::*;
use crate::run_with_args_in_dir;

fn run_mc(root: &Path, args: &[&str]) -> MonochangeResult<String> {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	let mut full_args = vec![std::ffi::OsString::from("monochange")];
	full_args.extend(args.iter().map(std::ffi::OsString::from));
	runtime.block_on(run_with_args_in_dir("monochange", full_args, root))
}

fn init_git_repo_with_origin(root: &Path, origin_url: &str) {
	ProcessCommand::new("git")
		.current_dir(root)
		.args(["init"])
		.output()
		.unwrap();
	ProcessCommand::new("git")
		.current_dir(root)
		.args(["remote", "add", "origin", origin_url])
		.output()
		.unwrap();
}

#[test]
fn normalize_and_derive_canonical_repo_id_handles_ssh_https_and_fallback() {
	assert_eq!(
		normalize_repo_identity("git@github.com:IfiokJr/Monochange.git"),
		"github.com/ifiokjr/monochange"
	);
	assert_eq!(
		normalize_repo_identity("https://github.com/IfiokJr/Monochange.git/"),
		"github.com/ifiokjr/monochange"
	);
	assert_eq!(
		normalize_repo_identity("ssh://git@github.com/IfiokJr/Monochange.git"),
		"github.com/ifiokjr/monochange"
	);

	let dir = tempdir().unwrap();
	let fallback_id = derive_canonical_repo_id(dir.path(), None);
	assert!(
		fallback_id.starts_with("local:"),
		"expected local fallback repo_id, got {fallback_id}"
	);

	init_git_repo_with_origin(dir.path(), "git@github.com:monochange/monochange.git");
	let git_id = derive_canonical_repo_id(dir.path(), None);
	assert_eq!(git_id, "github.com/monochange/monochange");

	let override_id =
		derive_canonical_repo_id(dir.path(), Some("https://gitlab.com/acme/repo.git"));
	assert_eq!(override_id, "gitlab.com/acme/repo");
}

#[test]
fn ci_auth_init_verify_token_and_rejects_cross_repo_or_tampered_tokens() {
	let repo_a = tempdir().unwrap();
	let repo_b = tempdir().unwrap();
	init_git_repo_with_origin(repo_a.path(), "https://github.com/acme/repo-a.git");
	init_git_repo_with_origin(repo_b.path(), "https://github.com/acme/repo-b.git");

	let init_out = run_mc(repo_a.path(), &["ci", "auth", "init", "--format", "json"]).unwrap();
	let init_report: CiAuthReport = serde_json::from_str(&init_out).unwrap();
	assert!(init_report.authenticated);
	assert_eq!(init_report.repo_id, "github.com/acme/repo-a");
	assert!(
		init_report
			.token
			.starts_with(&format!("mc_ci_{}_", init_report.repo_hash))
	);

	let auth_file = repo_a.path().join(CI_AUTH_RELATIVE_PATH);
	assert!(auth_file.is_file());
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let mode = fs::metadata(&auth_file).unwrap().permissions().mode() & 0o777;
		assert_eq!(mode, 0o600, "ci-auth.json should have 0600 permissions");
	}

	// Re-running init without --force must fail
	let duplicate_err = run_mc(repo_a.path(), &["ci", "auth", "init"]).unwrap_err();
	assert!(duplicate_err.to_string().contains("already exists"));

	// `monochange ci auth token` prints the active token
	let token_out = run_mc(repo_a.path(), &["ci", "auth", "token"]).unwrap();
	assert_eq!(token_out.trim(), init_report.token);

	// `monochange ci auth verify` succeeds for repo A
	let verify_out = run_mc(
		repo_a.path(),
		&[
			"ci",
			"auth",
			"verify",
			"--token",
			&init_report.token,
			"--format",
			"json",
		],
	)
	.unwrap();
	let verify_report: CiAuthReport = serde_json::from_str(&verify_out).unwrap();
	assert!(verify_report.authenticated);

	// Cross-repo rejection: init repo B, then try using repo A's token on repo B
	run_mc(repo_b.path(), &["ci", "auth", "init"]).unwrap();
	let cross_repo_err = run_mc(
		repo_b.path(),
		&["ci", "auth", "verify", "--token", &init_report.token],
	)
	.unwrap_err();
	assert!(
		cross_repo_err
			.to_string()
			.contains("cross-repo access denied")
			|| cross_repo_err.to_string().contains("repository mismatch"),
		"unexpected error: {cross_repo_err}"
	);

	// Tampered signature rejection on repo A
	let tampered_token = format!("{}00", &init_report.token[..init_report.token.len() - 2]);
	let tampered_err = run_mc(
		repo_a.path(),
		&["ci", "auth", "verify", "--token", &tampered_token],
	)
	.unwrap_err();
	assert!(
		tampered_err
			.to_string()
			.contains("HMAC signature verification failed")
	);
}

#[test]
fn ci_decide_evaluates_unconditional_overrides_and_max_skips_on_main() {
	let repo = tempdir().unwrap();
	init_git_repo_with_origin(repo.path(), "https://github.com/acme/monorepo.git");

	// 1. Docs-only change on PR should skip (`n`) for rust-tests
	let out_skip = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--branch",
			"feat/docs",
			"--changed-files",
			"docs/guide.md",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report_skip: CiDecideReport = serde_json::from_str(&out_skip).unwrap();
	assert_eq!(report_skip.decision, "n");
	assert!(!report_skip.run);
	assert!(!report_skip.unconditional);

	// 2. Unconditional via --unconditional flag
	let out_flag = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--unconditional",
			"--changed-files",
			"docs/guide.md",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report_flag: CiDecideReport = serde_json::from_str(&out_flag).unwrap();
	assert_eq!(report_flag.decision, "y");
	assert!(report_flag.run);
	assert!(report_flag.unconditional);

	// 3. Unconditional via schedule event
	let out_event = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--event",
			"schedule",
			"--changed-files",
			"docs/guide.md",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report_event: CiDecideReport = serde_json::from_str(&out_event).unwrap();
	assert_eq!(report_event.decision, "y");
	assert!(report_event.unconditional);

	// 4. Unconditional via release/* branch
	let out_branch = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--branch",
			"release/v1.2",
			"--changed-files",
			"docs/guide.md",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report_branch: CiDecideReport = serde_json::from_str(&out_branch).unwrap();
	assert_eq!(report_branch.decision, "y");
	assert!(report_branch.unconditional);

	// 5. Unconditional via [ci:full] commit message or label
	let out_msg = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--commit-message",
			"chore: update readme [ci:full]",
			"--changed-files",
			"docs/guide.md",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report_msg: CiDecideReport = serde_json::from_str(&out_msg).unwrap();
	assert_eq!(report_msg.decision, "y");
	assert!(report_msg.unconditional);

	// 6. Unconditional via Cargo.lock path
	let out_path = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--changed-files",
			"Cargo.lock",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report_path: CiDecideReport = serde_json::from_str(&out_path).unwrap();
	assert_eq!(report_path.decision, "y");
	assert!(report_path.unconditional);

	// 7. Consecutive skips on `main` reaching `--max-skips-on-main` (set to 2)
	for expected_skips in 1..=2 {
		let out_main_skip = run_mc(
			repo.path(),
			&[
				"ci",
				"decide",
				"--provider",
				"graph",
				"--job",
				"main-gate-job",
				"--branch",
				"main",
				"--max-skips-on-main",
				"2",
				"--changed-files",
				"docs/readme.md",
				"--format",
				"json",
			],
		)
		.unwrap();
		let rep: CiDecideReport = serde_json::from_str(&out_main_skip).unwrap();
		assert_eq!(rep.decision, "n");
		assert_eq!(rep.jobs[0].consecutive_skips_on_main, expected_skips);
	}

	// Next run on `main` after reaching 2 skips must force unconditional `y` and reset counter to 0
	let out_main_forced = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"main-gate-job",
			"--branch",
			"main",
			"--max-skips-on-main",
			"2",
			"--changed-files",
			"docs/readme.md",
			"--format",
			"json",
		],
	)
	.unwrap();
	let rep_forced: CiDecideReport = serde_json::from_str(&out_main_forced).unwrap();
	assert_eq!(rep_forced.decision, "y");
	assert!(rep_forced.unconditional);
	assert_eq!(rep_forced.jobs[0].consecutive_skips_on_main, 0);
}

#[test]
fn ci_verify_false_negative_learns_rule_and_forces_future_decide_to_y() {
	let repo = tempdir().unwrap();
	init_git_repo_with_origin(repo.path(), "https://github.com/acme/learn-repo.git");
	run_mc(repo.path(), &["ci", "auth", "init"]).unwrap();

	// Record a skipped decision (`n`) for `integration-suite` when `scripts/deploy.sh` was ignored by `--job-paths`
	let decide_initial = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"integration-suite",
			"--job-paths",
			"crates/**",
			"--changed-files",
			"scripts/deploy.sh",
			"--commit",
			"abc1234",
			"--format",
			"json",
		],
	)
	.unwrap();
	let rep_initial: CiDecideReport = serde_json::from_str(&decide_initial).unwrap();
	assert_eq!(rep_initial.decision, "n");

	// Now `verify` on `main` reports that `integration-suite` failed for commit `abc1234`!
	let verify_out = run_mc(
		repo.path(),
		&[
			"ci",
			"verify",
			"--job",
			"integration-suite",
			"--conclusion",
			"failure",
			"--duration-seconds",
			"1300",
			"--commit",
			"abc1234",
			"--changed-files",
			"scripts/deploy.sh",
			"--format",
			"json",
		],
	)
	.unwrap();
	let verify_rec: CiVerificationRecord = serde_json::from_str(&verify_out).unwrap();
	assert_eq!(
		verify_rec.classification,
		CiVerificationClassification::FalseNegative
	);
	assert!(verify_rec.learned_rule_added.is_some());
	assert!(verify_rec.duration_red_flag);
	assert_ne!(verify_rec.speed_suggestions, Vec::<String>::new());

	// Subsequent `decide` touching `scripts/deploy.sh` must now be forced to `y` (`unconditional: true`) via learned rule!
	let decide_after = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"integration-suite",
			"--job-paths",
			"crates/**",
			"--changed-files",
			"scripts/deploy.sh",
			"--format",
			"json",
		],
	)
	.unwrap();
	let rep_after: CiDecideReport = serde_json::from_str(&decide_after).unwrap();
	assert_eq!(rep_after.decision, "y");
	assert!(rep_after.run);
	assert!(rep_after.unconditional);
	assert_ne!(
		rep_after.jobs[0].matched_learned_rules,
		Vec::<String>::new()
	);
	// And because historical duration was 1300s (>20m threshold), decide also flags duration_red_flag!
	assert!(rep_after.jobs[0].duration_red_flag);
	assert_ne!(rep_after.jobs[0].speed_suggestions, Vec::<String>::new());
}

#[test]
fn ci_analytics_and_history_enforce_repo_auth_and_report_classifications() {
	let repo = tempdir().unwrap();
	init_git_repo_with_origin(repo.path(), "https://github.com/acme/analytics-repo.git");

	// Calling analytics before `ci auth init` must fail
	let uninit_err = run_mc(repo.path(), &["ci", "analytics"]).unwrap_err();
	assert!(
		uninit_err
			.to_string()
			.contains("authentication is not initialized")
	);

	// Initialize auth
	let auth_json = run_mc(repo.path(), &["ci", "auth", "init", "--format", "json"]).unwrap();
	let auth_report: CiAuthReport = serde_json::from_str(&auth_json).unwrap();

	// 1. False negative incident
	run_mc(
		repo.path(),
		&[
			"ci",
			"verify",
			"--job",
			"e2e",
			"--conclusion",
			"failure",
			"--prior-decision",
			"n",
			"--duration-seconds",
			"700",
			"--commit",
			"sha-fn",
			"--pr",
			"42",
			"--changed-files",
			"packages/web/index.ts",
		],
	)
	.unwrap();

	// 2. False positive incident (ran `y` on docs-only change and succeeded)
	run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"unit-tests",
			"--unconditional",
			"--changed-files",
			"docs/notes.md",
			"--commit",
			"sha-fp",
		],
	)
	.unwrap();
	run_mc(
		repo.path(),
		&[
			"ci",
			"verify",
			"--job",
			"unit-tests",
			"--conclusion",
			"success",
			"--duration-seconds",
			"300",
			"--commit",
			"sha-fp",
			"--changed-files",
			"docs/notes.md",
		],
	)
	.unwrap();

	// 3. True negative incident (skipped `n` and verified green)
	run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"lint-job",
			"--changed-files",
			"docs/notes.md",
			"--commit",
			"sha-tn",
		],
	)
	.unwrap();
	run_mc(
		repo.path(),
		&[
			"ci",
			"verify",
			"--job",
			"lint-job",
			"--conclusion",
			"success",
			"--duration-seconds",
			"120",
			"--commit",
			"sha-tn",
			"--changed-files",
			"docs/notes.md",
		],
	)
	.unwrap();

	// 4. True positive incident (ran `y` on code change and passed)
	run_mc(
		repo.path(),
		&[
			"ci",
			"verify",
			"--job",
			"build-job",
			"--conclusion",
			"success",
			"--prior-decision",
			"y",
			"--duration-seconds",
			"180",
			"--commit",
			"sha-tp",
			"--changed-files",
			"src/main.ts",
		],
	)
	.unwrap();

	// Reject invalid token on analytics and history
	let bad_token = issue_repo_token("https://github.com/other/repo.git", "wrong-secret");
	assert!(
		run_mc(repo.path(), &["ci", "analytics", "--token", &bad_token]).is_err(),
		"analytics must reject cross-repo token"
	);
	assert!(
		run_mc(repo.path(), &["ci", "history", "--token", &bad_token]).is_err(),
		"history must reject cross-repo token"
	);

	// Valid analytics report in JSON, Markdown, and Text
	let analytics_json = run_mc(
		repo.path(),
		&[
			"ci",
			"analytics",
			"--token",
			&auth_report.token,
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiAnalyticsReport = serde_json::from_str(&analytics_json).unwrap();
	assert!(report.authenticated);
	assert_eq!(report.total_decisions, 2);
	assert_eq!(report.run_decisions, 1);
	assert_eq!(report.skip_decisions, 1);
	assert_eq!(report.false_negatives, 1);
	assert_eq!(report.false_positives, 1);
	assert_eq!(report.true_negatives, 1);
	assert_eq!(report.true_positives, 1);
	assert_eq!(report.false_negative_incidents.len(), 1);
	assert_eq!(report.false_positive_incidents.len(), 1);
	assert!(!report.slow_jobs.is_empty(), "expected slow jobs");

	let analytics_md = run_mc(repo.path(), &["ci", "analytics", "--format", "markdown"]).unwrap();
	assert!(analytics_md.contains("# CI Gatekeeper Analytics"));
	assert!(analytics_md.contains("false_negative"));
	assert!(analytics_md.contains("false_positive"));

	let analytics_text = run_mc(repo.path(), &["ci", "analytics", "--format", "text"]).unwrap();
	assert!(analytics_text.contains("false_negatives"));
	assert!(analytics_text.contains("false_positives"));

	let history_json = run_mc(
		repo.path(),
		&["ci", "history", "--limit", "1", "--format", "json"],
	)
	.unwrap();
	let history: CiHistoryReport = serde_json::from_str(&history_json).unwrap();
	assert_eq!(history.decisions.len(), 1);
	assert_eq!(history.verifications.len(), 1);
}

#[test]
fn ci_decide_supports_opencode_go_and_jev_providers_and_github_outputs_format() {
	let server = MockServer::start();
	let opencode_mock = server.mock(|when, then| {
		when.method(POST).path("/v1/chat/completions");
		then.status(200).json_body(json!({
			"choices": [{
				"message": {
					"content": "{\"decision\":\"n\",\"reason\":\"opencode-go determined job is unaffected\"}"
				}
			}]
		}));
	});
	let jev_mock = server.mock(|when, then| {
		when.method(POST).path("/jev");
		then.status(200).json_body(json!({
			"decision": "y",
			"reason": "jev webhook requested execution"
		}));
	});

	let repo = tempdir().unwrap();
	init_git_repo_with_origin(repo.path(), "https://github.com/acme/provider-test.git");

	let out_opencode = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"opencode-go",
			"--api-url",
			&server.url("/v1/chat/completions"),
			"--api-key",
			"test-key",
			"--job",
			"api-tests",
			"--changed-files",
			"src/lib.rs",
			"--format",
			"json",
		],
	)
	.unwrap();
	opencode_mock.assert();
	let rep_opencode: CiDecideReport = serde_json::from_str(&out_opencode).unwrap();
	assert_eq!(rep_opencode.decision, "n");
	assert_eq!(rep_opencode.provider, "opencode-go");
	assert_eq!(rep_opencode.model, DEFAULT_OPENCODE_GO_MODEL);

	let out_jev = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"jev",
			"--api-url",
			&server.url("/jev"),
			"--job",
			"api-tests",
			"--changed-files",
			"src/lib.rs",
			"--format",
			"github-outputs",
		],
	)
	.unwrap();
	jev_mock.assert();
	assert!(out_jev.contains("decision=y"));
	assert!(out_jev.contains("run=true"));
	assert!(out_jev.contains("provider=jev"));
}
