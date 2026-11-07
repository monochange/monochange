#![allow(clippy::disallowed_methods)]

#[path = "ci_tests_support.rs"]
mod support;

use std::fs;
use std::path::Path;

use httpmock::Method::POST;
use httpmock::MockServer;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use serde_json::json;
use support::init_git_repo_with_origin;
use support::run_mc;
use tempfile::tempdir;

use super::*;

/// Successful HTTP status used by the mocked providers.
const HTTP_OK: u16 = 200;
/// Arguments that initialise auth and print JSON.
const AUTH_INIT_JSON_ARGS: &[&str] = &["ci", "auth", "init", "--format", "json"];

/// Arguments that request the newest history entry as JSON.
const HISTORY_LIMIT_JSON_ARGS: &[&str] = &["ci", "history", "--limit", "1", "--format", "json"];

/// Arguments that request analytics with an explicit token.
fn analytics_token_json_args(token: &str) -> [&str; 6] {
	["ci", "analytics", "--token", token, "--format", "json"]
}

/// Initialise auth in `repo` and return the parsed report.
fn init_auth(repo: &Path) -> CiAuthReport {
	let auth_json = run_mc(repo, AUTH_INIT_JSON_ARGS).unwrap();

	serde_json::from_str(&auth_json).unwrap()
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

/// Record a skipped job that later failed (false negative).
fn record_false_negative(repo: &Path) {
	run_mc(
		repo,
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
}

/// Record a docs-only run that passed (false positive).
fn record_false_positive(repo: &Path) {
	run_mc(
		repo,
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
		repo,
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
}

/// Record a skipped job that stayed green (true negative).
fn record_true_negative(repo: &Path) {
	run_mc(
		repo,
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
		repo,
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
}

/// Record a run on code that passed (true positive).
fn record_true_positive(repo: &Path) {
	run_mc(
		repo,
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
}

/// Record one incident of each verification classification.
fn record_classification_fixtures(repo: &Path) {
	record_false_negative(repo);
	record_false_positive(repo);
	record_true_negative(repo);
	record_true_positive(repo);
}

/// Assert analytics and history reject a token from another repository.
fn assert_foreign_token_rejected(repo: &Path, token: &str) {
	assert!(
		run_mc(repo, &["ci", "analytics", "--token", token]).is_err(),
		"analytics must reject cross-repo token"
	);
	assert!(
		run_mc(repo, &["ci", "history", "--token", token]).is_err(),
		"history must reject cross-repo token"
	);
}

/// Assert JSON, Markdown, and text analytics output for the fixtures.
fn assert_analytics_reports(repo: &Path, token: &str) {
	let analytics_json = run_mc(repo, &analytics_token_json_args(token)).unwrap();
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

	let analytics_md = run_mc(repo, &["ci", "analytics", "--format", "markdown"]).unwrap();

	assert!(analytics_md.contains("# CI Gatekeeper Analytics"));
	assert!(analytics_md.contains("false_negative"));
	assert!(analytics_md.contains("false_positive"));

	let analytics_text = run_mc(repo, &["ci", "analytics", "--format", "text"]).unwrap();

	assert!(analytics_text.contains("false_negatives"));
	assert!(analytics_text.contains("false_positives"));
}

/// Assert that `--limit 1` keeps the newest history entries.
fn assert_history_limit(repo: &Path) {
	let history_json = run_mc(repo, HISTORY_LIMIT_JSON_ARGS).unwrap();
	let history: CiHistoryReport = serde_json::from_str(&history_json).unwrap();

	assert_eq!(history.decisions.len(), 1);
	assert_eq!(history.verifications.len(), 1);
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

	let auth_report = init_auth(repo.path());

	record_classification_fixtures(repo.path());

	let bad_token = issue_repo_token("https://github.com/other/repo.git", "wrong-secret");

	assert_foreign_token_rejected(repo.path(), &bad_token);
	assert_analytics_reports(repo.path(), &auth_report.token);
	assert_history_limit(repo.path());
}

/// Register an `opencode-go` mock that skips the job.
fn mock_opencode_go(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(POST).path("/v1/chat/completions");
		then.status(HTTP_OK).json_body(json!({
			"choices": [{
				"message": {
					"content": "{\"decision\":\"n\",\"reason\":\"opencode-go determined job is unaffected\"}"
				}
			}]
		}));
	})
}

/// Register a JEV webhook mock that runs the job.
fn mock_jev(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(POST).path("/jev");
		then.status(HTTP_OK).json_body(json!({
			"decision": "y",
			"reason": "jev webhook requested execution"
		}));
	})
}

/// Run an `opencode-go` decision against the mock server.
fn run_opencode_go_decide(repo: &Path, server: &MockServer) -> CiDecideReport {
	let api_url = server.url("/v1/chat/completions");
	let output = run_mc(
		repo,
		&[
			"ci",
			"decide",
			"--provider",
			"opencode-go",
			"--api-url",
			&api_url,
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

	serde_json::from_str(&output).unwrap()
}

/// Run a JEV decision against the mock server and return the GitHub outputs.
fn run_jev_decide(repo: &Path, server: &MockServer) -> String {
	let api_url = server.url("/jev");

	run_mc(
		repo,
		&[
			"ci",
			"decide",
			"--provider",
			"jev",
			"--api-url",
			&api_url,
			"--job",
			"api-tests",
			"--changed-files",
			"src/lib.rs",
			"--format",
			"github-outputs",
		],
	)
	.unwrap()
}

#[test]
fn ci_decide_supports_opencode_go_and_jev_providers_and_github_outputs_format() {
	let server = MockServer::start();
	let opencode_mock = mock_opencode_go(&server);
	let jev_mock = mock_jev(&server);
	let repo = tempdir().unwrap();

	init_git_repo_with_origin(repo.path(), "https://github.com/acme/provider-test.git");

	let opencode_report = run_opencode_go_decide(repo.path(), &server);

	opencode_mock.assert();
	assert_eq!(opencode_report.decision, "n");
	assert_eq!(opencode_report.provider, "opencode-go");
	assert_eq!(opencode_report.model, DEFAULT_OPENCODE_GO_MODEL);

	let jev_output = run_jev_decide(repo.path(), &server);

	jev_mock.assert();
	assert!(jev_output.contains("decision=y"));
	assert!(jev_output.contains("run=true"));
	assert!(jev_output.contains("provider=jev"));
}

/// Write a two-crate cargo workspace where `app` depends on `core`.
fn write_graph_workspace(root: &Path) {
	for directory in ["crates/core/src", "crates/app/src"] {
		fs::create_dir_all(root.join(directory)).unwrap();
	}

	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/core\", \"crates/app\"]\nresolver = \"2\"\n",
	)
	.unwrap();
	fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap();
	fs::write(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n").unwrap();
	fs::write(
		root.join("crates/app/Cargo.toml"),
		"[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2021\"\n\n[dependencies]\ncore = { path = \"../core\" }\n",
	)
	.unwrap();
	fs::write(
		root.join("crates/app/src/lib.rs"),
		"pub fn app() {\n\tcore::core();\n}\n",
	)
	.unwrap();
	fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n\n[package.core]\npath = \"crates/core\"\n\n[package.app]\npath = \"crates/app\"\n",
	)
	.unwrap();
}

#[test]
fn graph_context_marks_configured_and_discovered_packages_affected() {
	let dir = tempdir().unwrap();
	let root = dir.path().canonicalize().unwrap();

	write_graph_workspace(&root);

	let graph =
		graph::evaluate_workspace_graph_context(&root, &["crates/core/src/lib.rs".to_string()]);

	assert!(!graph.docs_or_ignored_only);
	assert!(
		graph.affected_package_ids.contains("core"),
		"expected configured package `core`, got {:?}",
		graph.affected_package_ids
	);
	assert!(
		graph.affected_package_ids.contains("app"),
		"expected dependency propagation to `app`, got {:?}",
		graph.affected_package_ids
	);
	assert_eq!(
		graph.package_names_to_ids.get("core").map(String::as_str),
		Some("core")
	);
}

#[test]
fn graph_context_returns_empty_when_configuration_or_discovery_fails() {
	let dir = tempdir().unwrap();

	fs::write(dir.path().join("monochange.toml"), "[[[ not valid toml").unwrap();

	let graph = graph::evaluate_workspace_graph_context(
		dir.path(),
		&["crates/core/src/lib.rs".to_string()],
	);

	assert!(graph.affected_package_ids.is_empty());
	assert!(!graph.docs_or_ignored_only);
}

#[test]
fn graph_context_classifies_empty_and_doc_only_changes() {
	let dir = tempdir().unwrap();

	write_graph_workspace(dir.path());

	let empty = graph::evaluate_workspace_graph_context(dir.path(), &[]);

	assert!(empty.docs_or_ignored_only);
	assert!(empty.affected_package_ids.is_empty());

	let docs_only = graph::evaluate_workspace_graph_context(
		dir.path(),
		&[
			"docs/guide.md".to_string(),
			".changeset/acme.md".to_string(),
		],
	);

	assert!(docs_only.docs_or_ignored_only);
	assert!(docs_only.affected_package_ids.is_empty());
}

#[test]
fn graph_decision_covers_job_path_package_and_fallback_verdicts() {
	let mut graph = graph::WorkspaceGraphContext::default();

	graph.affected_package_ids.insert("core".to_string());
	graph.affected_package_ids.insert("app".to_string());
	graph
		.package_names_to_ids
		.insert("core".to_string(), "core".to_string());
	graph
		.package_names_to_ids
		.insert("@acme/web".to_string(), "app".to_string());
	graph
		.package_names_to_ids
		.insert("web".to_string(), "web".to_string());

	let files = vec!["crates/core/src/lib.rs".to_string()];
	let core_paths = vec!["crates/core/**".to_string()];
	let docs_paths = vec!["docs/**".to_string()];

	let (run, reason, packages) =
		graph::evaluate_graph_decision_for_job("core-tests", &[], &[], &[], &graph);

	assert!(!run);
	assert!(reason.contains("no changed files detected"));
	assert!(packages.is_empty());

	let (run, reason, packages) =
		graph::evaluate_graph_decision_for_job("core-tests", &files, &core_paths, &[], &graph);

	assert!(run);
	assert!(reason.contains("matched job path pattern `crates/core/**`"));
	assert_eq!(packages, vec!["app".to_string(), "core".to_string()]);

	let (run, reason, packages) =
		graph::evaluate_graph_decision_for_job("core-tests", &files, &docs_paths, &[], &graph);

	assert!(!run);
	assert!(reason.contains("no changed files matched configured `--job-paths`"));
	assert!(packages.is_empty());

	let (run, reason, packages) = graph::evaluate_graph_decision_for_job(
		"core-tests",
		&files,
		&[],
		&["core".to_string()],
		&graph,
	);

	assert!(run);
	assert!(reason.contains("matched job `core-tests`"));
	assert_eq!(packages, vec!["core".to_string()]);

	let (run, _reason, packages) = graph::evaluate_graph_decision_for_job(
		"core-tests",
		&files,
		&[],
		&["@acme/web".to_string()],
		&graph,
	);

	assert!(run);
	assert_eq!(packages, vec!["app".to_string()]);

	let (run, reason, packages) = graph::evaluate_graph_decision_for_job(
		"core-tests",
		&files,
		&[],
		&["missing".to_string()],
		&graph,
	);

	assert!(!run);
	assert!(reason.contains("none of the job packages"));
	assert!(packages.is_empty());

	let mut docs_graph = graph.clone();

	docs_graph.docs_or_ignored_only = true;

	let (run, reason, _) =
		graph::evaluate_graph_decision_for_job("docs-build", &files, &[], &[], &docs_graph);

	assert!(run);
	assert!(reason.contains("matched documentation-only changes"));

	let (run, reason, _) =
		graph::evaluate_graph_decision_for_job("rust-tests", &files, &[], &[], &docs_graph);

	assert!(!run);
	assert!(reason.contains("only documentation or non-code metadata files changed"));

	for job in [
		"core",
		"@acme/web",
		"core:tests",
		"tests:core",
		"app:lint",
		"lint:app",
	] {
		let (run, reason, packages) =
			graph::evaluate_graph_decision_for_job(job, &files, &[], &[], &graph);

		assert!(run, "expected `{job}` to run: {reason}");
		assert!(!packages.is_empty());
	}

	let (run, reason, packages) =
		graph::evaluate_graph_decision_for_job("web:tests", &files, &[], &[], &graph);

	assert!(!run);
	assert!(reason.contains("was not affected by the changed files"));
	assert!(packages.is_empty());

	let (run, reason, packages) =
		graph::evaluate_graph_decision_for_job("unscoped", &files, &[], &[], &graph);

	assert!(run);
	assert!(reason.contains("workspace graph detected 2 affected package(s)"));
	assert_eq!(packages, vec!["app".to_string(), "core".to_string()]);

	let (run, reason, _) = graph::evaluate_graph_decision_for_job(
		"unscoped",
		&files,
		&[],
		&[],
		&graph::WorkspaceGraphContext::default(),
	);

	assert!(run);
	assert!(reason.contains("running `unscoped` conservatively"));
}

#[test]
fn graph_package_dir_helpers_cover_root_and_outside_workspace_records() {
	let dir = tempdir().unwrap();
	let root = dir.path();
	let inside = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.to_path_buf(),
		None,
		PublishState::Public,
	);
	let at_root = PackageRecord::new(
		Ecosystem::Cargo,
		"root",
		root.join("Cargo.toml"),
		root.to_path_buf(),
		None,
		PublishState::Public,
	);
	let outside = PackageRecord::new(
		Ecosystem::Cargo,
		"outside",
		root.join("..").join("outside/Cargo.toml"),
		root.to_path_buf(),
		None,
		PublishState::Public,
	);

	assert_eq!(graph::relative_package_dir(&inside, root), "crates/core");
	assert_eq!(graph::relative_package_dir(&at_root, root), "");
	assert_eq!(graph::relative_package_dir(&outside, root), "../outside");
	assert_eq!(
		graph::relative_package_dir(&inside, &root.join("nested")),
		""
	);

	assert!(graph::package_dir_matches("crates/core", "crates/core"));
	assert!(graph::package_dir_matches(
		"crates/core",
		"crates/core/src/lib.rs"
	));
	assert!(!graph::package_dir_matches(
		"crates/core",
		"crates/core-extra/src/lib.rs"
	));
	assert!(!graph::package_dir_matches("", "crates/core/src/lib.rs"));
	assert!(!graph::package_dir_matches("crates/core", "docs/guide.md"));
}

#[test]
fn detect_changed_files_and_diff_specifications_use_git() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/diff-repo.git");
	fs::write(root.join("first.txt"), "first\n").unwrap();
	git_commit_all(root, "first");
	fs::write(root.join("second.txt"), "second\n").unwrap();
	git_commit_all(root, "second");

	let both = graph::detect_changed_files_from_git(root, Some("HEAD~1"), Some("HEAD"));

	assert_eq!(both, vec!["second.txt".to_string()]);

	let base_only = graph::detect_changed_files_from_git(root, Some("HEAD~1"), None);

	assert_eq!(base_only, vec!["second.txt".to_string()]);

	let head_only = graph::detect_changed_files_from_git(root, None, Some("HEAD"));

	assert!(head_only.is_empty());
	assert_eq!(
		graph::detect_changed_files_from_git(root, None, None),
		Vec::<String>::new()
	);

	assert_eq!(graph::diff_specification(Some("a"), Some("b")), "a...b");
	assert_eq!(graph::diff_specification(Some("a"), None), "a");
	assert_eq!(graph::diff_specification(None, Some("b")), "b");
	assert_eq!(graph::diff_specification(None, None), "HEAD");
}

#[test]
fn detect_changed_files_returns_empty_outside_a_git_repository() {
	let dir = tempdir().unwrap();

	assert!(graph::detect_changed_files_from_git(dir.path(), Some("HEAD"), None).is_empty());
}

#[test]
fn derived_learned_patterns_cover_package_roots_and_fallbacks() {
	assert_eq!(
		graph::derive_learned_patterns_from_files(&[]),
		vec!["*".to_string()]
	);
	assert_eq!(
		graph::derive_learned_patterns_from_files(&[String::new()]),
		vec!["*".to_string()]
	);

	let derived = graph::derive_learned_patterns_from_files(&[
		"crates/core/src/lib.rs".to_string(),
		"src/main.rs".to_string(),
	]);

	assert_eq!(
		derived,
		vec![
			"crates/core/*".to_string(),
			"crates/core/**".to_string(),
			"crates/core/src/lib.rs".to_string(),
			"src/*".to_string(),
			"src/main.rs".to_string(),
		]
	);
}

/// Stage every file in `root` and commit it with `message`.
fn git_commit_all(root: &Path, message: &str) {
	for args in [
		vec!["config", "user.name", "monochange"],
		vec!["config", "user.email", "monochange@example.com"],
		vec!["add", "-A"],
		vec!["commit", "-m", message],
	] {
		let status = std::process::Command::new("git")
			.current_dir(root)
			.args(["-c", "commit.gpgsign=false"])
			.args(&args)
			.output()
			.unwrap();

		assert!(
			status.status.success(),
			"git {args:?} failed: {}",
			String::from_utf8_lossy(&status.stderr)
		);
	}
}

/// Initialise a bare git repository in `root`.
fn git_init(root: &Path) {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(["init", "-q"])
		.output()
		.unwrap();

	assert!(output.status.success());
}

#[test]
fn auth_normalizes_remote_identities_and_schemes() {
	assert_eq!(
		normalize_repo_identity("https://github.com"),
		"https://github.com"
	);
	assert_eq!(normalize_repo_identity("git@github.com"), "git@github.com");
	assert_eq!(
		normalize_repo_identity("http://github.com/Acme/Repo.git/"),
		"github.com/acme/repo"
	);
	assert_eq!(
		normalize_repo_identity("git://github.com/Acme/Repo.git"),
		"github.com/acme/repo"
	);
	assert_eq!(
		normalize_repo_identity("ssh://git@github.com/acme/Repo.git"),
		"github.com/acme/repo"
	);
	assert_eq!(
		normalize_repo_identity("github.com/Acme/Repo.git"),
		"github.com/acme/repo"
	);
	assert_eq!(
		normalize_repo_identity("  https://GitHub.com/Acme/Repo.git/  "),
		"github.com/acme/repo"
	);
}

#[test]
fn auth_file_errors_cover_unreadable_unparsable_and_foreign_files() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	fs::create_dir_all(auth::ci_auth_file_path(root)).unwrap();

	let read_error =
		authenticate_repo_access(root, Some("github.com/acme/auth"), None).unwrap_err();

	assert!(
		read_error.to_string().contains("failed to read"),
		"unexpected error: {read_error}"
	);

	fs::remove_dir(auth::ci_auth_file_path(root)).unwrap();
	fs::create_dir_all(root.join(".monochange")).unwrap();
	fs::write(auth::ci_auth_file_path(root), "{ not json").unwrap();

	let parse_error =
		authenticate_repo_access(root, Some("github.com/acme/auth"), None).unwrap_err();

	assert!(
		parse_error.to_string().contains("failed to parse"),
		"unexpected error: {parse_error}"
	);

	let foreign = CiAuthFile {
		schema_version: auth::AUTH_SCHEMA_VERSION,
		repo_id: "github.com/other/repo".to_string(),
		repo_hash: compute_repo_hash("github.com/other/repo"),
		secret: "other-secret".to_string(),
		token: issue_repo_token("github.com/other/repo", "other-secret"),
		created_at: "2026-01-01T00:00:00Z".to_string(),
	};

	fs::write(
		auth::ci_auth_file_path(root),
		serde_json::to_string(&foreign).unwrap(),
	)
	.unwrap();

	let foreign_error =
		authenticate_repo_access(root, Some("github.com/acme/auth"), None).unwrap_err();

	assert!(
		foreign_error
			.to_string()
			.contains("belongs to `github.com/other/repo`"),
		"unexpected error: {foreign_error}"
	);

	let blocked = tempdir().unwrap();

	fs::write(blocked.path().join(".monochange"), "not a directory").unwrap();

	let create_error = auth::save_ci_auth_file(blocked.path(), &foreign).unwrap_err();

	assert!(
		create_error.to_string().contains("failed to create"),
		"unexpected error: {create_error}"
	);

	let write_blocked = tempdir().unwrap();

	fs::create_dir_all(auth::ci_auth_file_path(write_blocked.path())).unwrap();

	let write_error = auth::save_ci_auth_file(write_blocked.path(), &foreign).unwrap_err();

	assert!(
		write_error.to_string().contains("failed to write"),
		"unexpected error: {write_error}"
	);
}

#[test]
fn auth_worktree_git_config_resolves_absolute_relative_and_common_configs() {
	let dir = tempdir().unwrap();
	let direct_root = dir.path().join("direct-checkout");

	fs::create_dir_all(&direct_root).unwrap();

	let direct_git_dir = dir.path().join("direct-git-dir");

	fs::create_dir_all(&direct_git_dir).unwrap();
	fs::write(
		direct_git_dir.join("config"),
		"[remote \"origin\"]\n\turl = https://github.com/Acme/Direct.git\n",
	)
	.unwrap();
	fs::write(
		direct_root.join(".git"),
		format!("gitdir: {}\n", direct_git_dir.display()),
	)
	.unwrap();

	assert_eq!(
		derive_canonical_repo_id(&direct_root, None),
		"github.com/acme/direct"
	);

	let relative_root = dir.path().join("relative-checkout");

	fs::create_dir_all(relative_root.join("nested/git-dir")).unwrap();
	fs::write(
		relative_root.join("nested/git-dir/config"),
		"[remote \"origin\"]\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n\turl = git@github.com:Acme/Relative.git\n",
	)
	.unwrap();
	fs::write(relative_root.join(".git"), "gitdir: nested/git-dir\n").unwrap();

	assert_eq!(
		derive_canonical_repo_id(&relative_root, None),
		"github.com/acme/relative"
	);

	let common_git = dir.path().join("common/.git");
	let common_branch = common_git.join("worktrees/branch");
	let linked_root = dir.path().join("linked-worktree");

	fs::create_dir_all(&common_branch).unwrap();
	fs::create_dir_all(&linked_root).unwrap();
	fs::write(
		common_git.join("config"),
		"[remote \"origin\"]\n\turl = https://github.com/Acme/Common.git\n",
	)
	.unwrap();
	fs::write(
		linked_root.join(".git"),
		format!("gitdir: {}\n", common_branch.display()),
	)
	.unwrap();

	assert_eq!(
		derive_canonical_repo_id(&linked_root, None),
		"github.com/acme/common"
	);

	let missing_pointer = dir.path().join("missing-pointer");

	fs::create_dir_all(&missing_pointer).unwrap();
	fs::write(missing_pointer.join(".git"), "no pointer here\n").unwrap();

	assert!(
		derive_canonical_repo_id(&missing_pointer, None).starts_with("local:"),
		"expected local fallback"
	);

	let orphan_worktree = dir.path().join("orphan-worktree");
	let missing_gitdir = dir.path().join("orphan-git/worktrees/branch");

	fs::create_dir_all(&orphan_worktree).unwrap();
	fs::write(
		orphan_worktree.join(".git"),
		format!("gitdir: {}\n", missing_gitdir.display()),
	)
	.unwrap();

	assert!(
		derive_canonical_repo_id(&orphan_worktree, None).starts_with("local:"),
		"expected local fallback for a worktree without any config file"
	);
}

#[test]
fn auth_falls_back_to_the_git_cli_when_the_config_file_has_no_direct_origin() {
	let included = tempdir().unwrap();

	git_init(included.path());
	fs::write(
		included.path().join(".git/config"),
		"[include]\n\tpath = extra.conf\n",
	)
	.unwrap();
	fs::write(
		included.path().join(".git/extra.conf"),
		"[remote \"origin\"]\n\turl = https://github.com/Acme/Included.git\n",
	)
	.unwrap();

	assert_eq!(
		derive_canonical_repo_id(included.path(), None),
		"github.com/acme/included"
	);

	let blank = tempdir().unwrap();

	git_init(blank.path());
	fs::write(
		blank.path().join(".git/config"),
		"[remote \"origin\"]\n\turl = \"\"\n",
	)
	.unwrap();

	assert!(derive_canonical_repo_id(blank.path(), None).starts_with("local:"));

	let no_origin = tempdir().unwrap();

	git_init(no_origin.path());
	assert!(derive_canonical_repo_id(no_origin.path(), None).starts_with("local:"));
}

#[test]
fn auth_token_and_secret_helpers_report_format_and_encoding_errors() {
	let repo = "https://github.com/acme/token-repo.git";
	let secret = "shhh";
	let token = issue_repo_token(repo, secret);

	verify_repo_token(&token, repo, secret).unwrap();

	let missing_prefix = verify_repo_token("nope", repo, secret).unwrap_err();

	assert!(
		missing_prefix
			.to_string()
			.contains("expected token starting with `mc_ci_`"),
		"unexpected error: {missing_prefix}"
	);

	let missing_underscore = verify_repo_token("mc_ci_abc", repo, secret).unwrap_err();

	assert!(
		missing_underscore
			.to_string()
			.contains("expected `mc_ci_<repo_hash>_<hmac>`"),
		"unexpected error: {missing_underscore}"
	);

	let repo_hash = compute_repo_hash(repo);
	let odd = verify_repo_token(&format!("mc_ci_{repo_hash}_abc"), repo, secret).unwrap_err();

	assert!(
		odd.to_string()
			.contains("invalid CI auth token signature encoding"),
		"unexpected error: {odd}"
	);

	let invalid = verify_repo_token(&format!("mc_ci_{repo_hash}_zz"), repo, secret).unwrap_err();

	assert!(
		invalid
			.to_string()
			.contains("invalid CI auth token signature encoding"),
		"unexpected error: {invalid}"
	);

	let cross_repo = verify_repo_token(
		&issue_repo_token("https://github.com/acme/other.git", secret),
		repo,
		secret,
	)
	.unwrap_err();

	assert!(
		cross_repo.to_string().contains("cross-repo access denied"),
		"unexpected error: {cross_repo}"
	);
}

#[test]
fn auth_memory_store_errors_cover_corrupt_missing_and_foreign_stores() {
	let dir = tempdir().unwrap();
	let root = dir.path();
	let repo_id = "github.com/acme/memory-repo";

	let store = auth::load_ci_memory_store(root, Some(repo_id), false, None).unwrap();

	assert_eq!(store.repo_id, repo_id);

	auth::save_ci_memory_store(root, &store).unwrap();

	assert_eq!(
		auth::load_ci_memory_store(root, Some(repo_id), false, None).unwrap(),
		store
	);

	let memory_path = root.join(CI_MEMORY_RELATIVE_PATH);

	fs::write(&memory_path, "{ not json").unwrap();

	let parse_error = auth::load_ci_memory_store(root, Some(repo_id), false, None).unwrap_err();

	assert!(
		parse_error.to_string().contains("failed to parse"),
		"unexpected error: {parse_error}"
	);

	fs::remove_file(&memory_path).unwrap();
	fs::create_dir(&memory_path).unwrap();

	let read_error = auth::load_ci_memory_store(root, Some(repo_id), false, None).unwrap_err();

	assert!(
		read_error.to_string().contains("failed to read"),
		"unexpected error: {read_error}"
	);

	fs::remove_dir(&memory_path).unwrap();

	let mut foreign = store.clone();

	foreign.repo_id = "github.com/other/repo".to_string();
	foreign.repo_hash = compute_repo_hash(&foreign.repo_id);
	fs::write(&memory_path, serde_json::to_string(&foreign).unwrap()).unwrap();

	let mismatch = auth::load_ci_memory_store(root, Some(repo_id), false, None).unwrap_err();

	assert!(
		mismatch
			.to_string()
			.contains("CI memory repository mismatch"),
		"unexpected error: {mismatch}"
	);

	let blocked = tempdir().unwrap();

	fs::write(blocked.path().join(".monochange"), "not a directory").unwrap();

	let create_error = auth::save_ci_memory_store(blocked.path(), &store).unwrap_err();

	assert!(
		create_error.to_string().contains("failed to create"),
		"unexpected error: {create_error}"
	);

	let write_blocked = tempdir().unwrap();

	fs::create_dir_all(write_blocked.path().join(CI_MEMORY_RELATIVE_PATH)).unwrap();

	let write_error = auth::save_ci_memory_store(write_blocked.path(), &store).unwrap_err();

	assert!(
		write_error.to_string().contains("failed to write"),
		"unexpected error: {write_error}"
	);
}

#[test]
fn ci_subcommand_dispatch_rejects_missing_selections() {
	let dir = tempdir().unwrap();
	let ci_command = clap::Command::new("ci").subcommand(clap::Command::new("auth"));
	let ci_matches = ci_command.try_get_matches_from(["ci"]).unwrap();
	let ci_error = block_on(run_ci_subcommand(dir.path(), &ci_matches)).unwrap_err();

	assert!(
		ci_error
			.to_string()
			.contains("missing `monochange ci` subcommand"),
		"unexpected error: {ci_error}"
	);

	let auth_matches = clap::Command::new("ci")
		.subcommand(clap::Command::new("auth"))
		.try_get_matches_from(["ci", "auth"])
		.unwrap();
	let auth_error = block_on(run_ci_subcommand(dir.path(), &auth_matches)).unwrap_err();

	assert!(
		auth_error
			.to_string()
			.contains("missing `monochange ci auth` subcommand"),
		"unexpected error: {auth_error}"
	);
}

/// Block on a `ci` future using the shared test runtime helper.
fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
	crate::tests::block_on_in_context(future)
}

#[test]
fn decide_defaults_job_and_detects_changed_files_from_git() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/default-job.git");
	fs::write(root.join("README.md"), "one\n").unwrap();
	git_commit_all(root, "first");
	fs::write(root.join("README.md"), "two\n").unwrap();
	git_commit_all(root, "second");

	let output = run_mc(
		root,
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--base",
			"HEAD~1",
			"--head",
			"HEAD",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();

	assert_eq!(report.jobs.len(), 1);
	assert_eq!(report.jobs[0].job, "default");
	assert_eq!(report.jobs[0].changed_files, vec!["README.md".to_string()]);
	assert_eq!(report.decision, "n");
}

#[test]
fn decide_unconditional_ref_labels_and_custom_patterns_force_runs() {
	let dir = tempdir().unwrap();

	init_git_repo_with_origin(dir.path(), "https://github.com/acme/unconditional.git");

	let output = run_mc(
		dir.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"release-job",
			"--ref",
			"refs/tags/v1.0.0",
			"--label",
			"[ci:full]",
			"--unconditional-paths",
			"custom/*",
			"--changed-files",
			"docs/guide.md",
			"--duration-seconds",
			"2000",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();
	let reasons = report.jobs[0].unconditional_reasons.join("\n");

	assert!(report.unconditional);
	assert!(report.jobs[0].duration_red_flag);
	assert!(reasons.contains("ref `refs/tags/v1.0.0` matches unconditional branch/ref pattern"));
	assert!(reasons.contains("label `[ci:full]` forces unconditional CI"));

	let text = run_mc(
		dir.path(),
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"rust-tests",
			"--changed-files",
			"crates/core/src/lib.rs",
			"--duration-seconds",
			"2000",
		],
	)
	.unwrap();

	assert!(
		text.contains("speed suggestion:"),
		"unexpected text: {text}"
	);
}

#[test]
fn decide_falls_back_to_the_graph_when_external_providers_fail() {
	let dir = tempdir().unwrap();

	init_git_repo_with_origin(dir.path(), "https://github.com/acme/fallback.git");

	for provider in ["opencode-go", "openrouter", "cloudflare", "jev"] {
		let output = run_mc(
			dir.path(),
			&[
				"ci",
				"decide",
				"--provider",
				provider,
				"--api-url",
				"http://127.0.0.1:1/v1/chat/completions",
				"--api-key",
				"test-key",
				"--job",
				"fallback-job",
				"--changed-files",
				"crates/core/src/lib.rs",
				"--format",
				"json",
			],
		)
		.unwrap();
		let report: CiDecideReport = serde_json::from_str(&output).unwrap();

		assert_eq!(
			report.provider, "graph",
			"provider {provider} did not fall back"
		);
		assert!(
			report.reason.contains("deterministic graph fallback"),
			"unexpected reason for {provider}: {}",
			report.reason
		);
		assert!(report.run);
	}
}

#[test]
fn provider_payloads_cover_text_json_nested_and_error_responses() {
	assert_eq!(
		provider::parse_provider_decision_payload("y").unwrap(),
		(true, "provider returned `y`".to_string())
	);
	assert!(provider::parse_provider_decision_payload("YES").unwrap().0);
	assert_eq!(
		provider::parse_provider_decision_payload("no").unwrap(),
		(false, "provider returned `n`".to_string())
	);
	assert!(
		!provider::parse_provider_decision_payload("false")
			.unwrap()
			.0
	);

	let run = provider::parse_provider_decision_payload("{\"decision\":\"run\"}").unwrap();

	assert!(run.0);
	assert_eq!(run.1, "provider decided to run job");

	let skip = provider::parse_provider_decision_payload("{\"run\":false}").unwrap();

	assert!(!skip.0);
	assert_eq!(skip.1, "provider decided to skip job");

	let reasoned =
		provider::parse_provider_decision_payload("{\"decision\":\"y\",\"reason\":\"because\"}")
			.unwrap();

	assert_eq!(reasoned.1, "because");

	let nested = provider::parse_provider_decision_payload(
		"{\"choices\":[{\"message\":{\"content\":\"n\"}}]}",
	)
	.unwrap();

	assert!(!nested.0);

	let missing = provider::parse_provider_decision_payload("{\"other\":true}").unwrap_err();

	assert!(
		missing.contains("did not include `decision`"),
		"unexpected error: {missing}"
	);

	let invalid = provider::parse_provider_decision_payload("not json").unwrap_err();

	assert!(
		invalid.contains("invalid JSON response"),
		"unexpected error: {invalid}"
	);
}

#[test]
fn provider_requests_send_bearer_auth_and_report_http_failures() {
	let server = MockServer::start();
	let jev_mock = server.mock(|when, then| {
		when.method(POST)
			.path("/jev")
			.header("authorization", "Bearer secret");
		then.status(HTTP_OK)
			.json_body(json!({"decision": "y", "reason": "jev approved"}));
	});
	let chat_mock = server.mock(|when, then| {
		when.method(POST).path("/chat");
		then.status(HTTP_OK)
			.json_body(json!({"choices": [{"message": {"content": "n"}}]}));
	});
	let failure_mock = server.mock(|when, then| {
		when.method(POST).path("/fail");
		then.status(500).body("boom");
	});

	let changed_files = vec!["src/lib.rs".to_string()];
	let affected_packages = vec!["core".to_string()];
	let prompt = provider::DecisionPrompt {
		job: "api-tests",
		changed_files: &changed_files,
		affected_packages: &affected_packages,
	};

	let jev_url = server.url("/jev");
	let jev_overrides = provider::ProviderOverrides {
		model: Some("custom-model"),
		api_url: Some(&jev_url),
		api_key: Some("secret"),
		account_id: None,
	};
	let jev_outcome = block_on(provider::query_external_decision_provider(
		CiDecisionProvider::Jev,
		&jev_overrides,
		&prompt,
	))
	.unwrap();

	jev_mock.assert();
	assert!(jev_outcome.run);
	assert_eq!(jev_outcome.provider_used, "jev");
	assert_eq!(jev_outcome.model_used, "custom-model");

	let chat_url = server.url("/chat");
	let chat_overrides = provider::ProviderOverrides {
		model: None,
		api_url: Some(&chat_url),
		api_key: Some(""),
		account_id: None,
	};
	let chat_outcome = block_on(provider::query_external_decision_provider(
		CiDecisionProvider::OpenaiCompatible,
		&chat_overrides,
		&prompt,
	))
	.unwrap();

	chat_mock.assert();
	assert!(!chat_outcome.run);
	assert_eq!(chat_outcome.provider_used, "openai-compatible");

	let failure_url = server.url("/fail");
	let failure_overrides = provider::ProviderOverrides {
		model: None,
		api_url: Some(&failure_url),
		api_key: Some("key"),
		account_id: None,
	};
	let failure = block_on(provider::query_external_decision_provider(
		CiDecisionProvider::OpenaiCompatible,
		&failure_overrides,
		&prompt,
	))
	.unwrap_err();

	failure_mock.assert();
	assert!(
		failure.contains("returned HTTP 500"),
		"unexpected error: {failure}"
	);

	let graph_overrides = provider::ProviderOverrides {
		model: None,
		api_url: None,
		api_key: None,
		account_id: None,
	};
	let graph_error = block_on(provider::query_external_decision_provider(
		CiDecisionProvider::Graph,
		&graph_overrides,
		&prompt,
	))
	.unwrap_err();

	assert_eq!(graph_error, "graph provider is local");
}

#[test]
fn provider_endpoint_resolution_covers_every_chat_provider() {
	temp_env::with_vars(
		[
			("OPENCODE_GO_API_KEY", None::<&str>),
			("OPENCODE_GO_API_URL", None),
			("OPENROUTER_API_KEY", None),
			("OPENROUTER_API_URL", None),
			("CLOUDFLARE_API_TOKEN", None),
			("CLOUDFLARE_API_URL", None),
			("CLOUDFLARE_ACCOUNT_ID", None),
			("OPENAI_API_URL", None),
			("OPENAI_API_KEY", None),
		],
		assert_provider_endpoint_resolution,
	);
}

/// Assert endpoint resolution with every provider environment variable cleared.
fn assert_provider_endpoint_resolution() {
	let with_key = provider::ProviderOverrides {
		model: None,
		api_url: None,
		api_key: Some("key"),
		account_id: Some("acc123"),
	};

	let opencode =
		provider::resolve_chat_endpoint(provider::ChatProvider::OpencodeGo, &with_key).unwrap();

	assert_eq!(opencode.url, DEFAULT_OPENCODE_GO_API_URL);
	assert_eq!(opencode.api_key, "key");
	assert_eq!(opencode.default_model, DEFAULT_OPENCODE_GO_MODEL);

	let cloudflare =
		provider::resolve_chat_endpoint(provider::ChatProvider::Cloudflare, &with_key).unwrap();

	assert_eq!(
		cloudflare.url,
		"https://api.cloudflare.com/client/v4/accounts/acc123/ai/v1/chat/completions"
	);
	assert_eq!(cloudflare.default_model, DEFAULT_CLOUDFLARE_MODEL);

	let custom = provider::ProviderOverrides {
		model: Some("custom"),
		api_url: Some("http://localhost:9/v1/chat/completions"),
		api_key: Some("key"),
		account_id: None,
	};

	assert_eq!(
		provider::resolve_chat_endpoint(provider::ChatProvider::Openrouter, &custom)
			.unwrap()
			.url,
		"http://localhost:9/v1/chat/completions"
	);

	let compatible =
		provider::resolve_chat_endpoint(provider::ChatProvider::OpenaiCompatible, &custom).unwrap();

	assert_eq!(compatible.url, "http://localhost:9/v1/chat/completions");
	assert_eq!(compatible.default_model, DEFAULT_OPENCODE_GO_MODEL);

	let missing_key = provider::ProviderOverrides {
		model: None,
		api_url: Some("http://localhost:9"),
		api_key: None,
		account_id: None,
	};

	assert!(
		provider::resolve_chat_endpoint(provider::ChatProvider::OpencodeGo, &missing_key).is_err()
	);
	assert!(
		provider::resolve_chat_endpoint(provider::ChatProvider::Openrouter, &missing_key).is_err()
	);
	assert!(
		provider::resolve_chat_endpoint(provider::ChatProvider::Cloudflare, &missing_key).is_err()
	);

	let no_url = provider::ProviderOverrides {
		model: None,
		api_url: None,
		api_key: Some("key"),
		account_id: None,
	};

	assert!(
		provider::resolve_chat_endpoint(provider::ChatProvider::OpenaiCompatible, &no_url).is_err()
	);
}

/// Run `ci verify` with `args` and parse the JSON record.
fn verify_record(root: &Path, args: &[&str]) -> CiVerificationRecord {
	let mut full_args = vec!["ci", "verify"];

	full_args.extend_from_slice(args);
	full_args.extend_from_slice(&["--format", "json"]);

	let output = run_mc(root, &full_args).unwrap();

	serde_json::from_str(&output).unwrap()
}

#[test]
fn verify_classifies_outcomes_from_prior_decisions_and_reuses_learned_rules() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/verify-repo.git");
	run_mc(root, &["ci", "auth", "init"]).unwrap();

	let code = verify_record(
		root,
		&[
			"--job",
			"code-job",
			"--conclusion",
			"success",
			"--changed-files",
			"crates/core/src/lib.rs",
		],
	);

	assert_eq!(code.prior_decision, "y");
	assert_eq!(
		code.classification,
		CiVerificationClassification::TruePositive
	);

	let docs = verify_record(
		root,
		&[
			"--job",
			"prose-job",
			"--conclusion",
			"success",
			"--changed-files",
			"docs/guide.md",
		],
	);

	assert_eq!(docs.prior_decision, "n");
	assert_eq!(
		docs.classification,
		CiVerificationClassification::TrueNegative
	);

	let orphan = verify_record(root, &["--job", "orphan-job", "--conclusion", "success"]);

	assert!(orphan.changed_files.is_empty());
	assert_eq!(
		orphan.classification,
		CiVerificationClassification::TrueNegative
	);

	let caught = verify_record(
		root,
		&[
			"--job",
			"caught-job",
			"--conclusion",
			"failure",
			"--prior-decision",
			"y",
			"--changed-files",
			"crates/core/src/lib.rs",
		],
	);

	assert_eq!(
		caught.classification,
		CiVerificationClassification::TruePositive
	);

	run_mc(
		root,
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"forced-job",
			"--unconditional",
			"--changed-files",
			"docs/guide.md",
			"--commit",
			"forced-c1",
		],
	)
	.unwrap();

	let forced = verify_record(
		root,
		&[
			"--job",
			"forced-job",
			"--conclusion",
			"failure",
			"--commit",
			"forced-c1",
		],
	);

	assert_eq!(forced.prior_decision, "n");
	assert_eq!(
		forced.classification,
		CiVerificationClassification::FalseNegative
	);
	assert_eq!(forced.changed_files, vec!["docs/guide.md".to_string()]);

	let forced_rule = forced.learned_rule_added.clone().unwrap();
	let repeated = verify_record(
		root,
		&[
			"--job",
			"forced-job",
			"--conclusion",
			"failure",
			"--commit",
			"forced-c1",
		],
	);

	assert_eq!(repeated.learned_rule_added.unwrap().id, forced_rule.id);

	let history_output = run_mc(
		root,
		&["ci", "history", "--job", "forced-job", "--format", "json"],
	)
	.unwrap();
	let history: CiHistoryReport = serde_json::from_str(&history_output).unwrap();

	assert_eq!(history.learned_rules.len(), 1);

	let text = run_mc(
		root,
		&[
			"ci",
			"verify",
			"--job",
			"text-job",
			"--conclusion",
			"success",
			"--changed-files",
			"docs/guide.md",
		],
	)
	.unwrap();

	assert!(
		text.contains("classification: true_negative"),
		"unexpected: {text}"
	);
}

#[test]
fn analytics_uses_fallback_minutes_for_unmeasured_durations() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/analytics-fallback.git");
	run_mc(root, &["ci", "auth", "init"]).unwrap();
	run_mc(
		root,
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"unmeasured",
			"--changed-files",
			"docs/a.md",
		],
	)
	.unwrap();
	run_mc(
		root,
		&[
			"ci",
			"decide",
			"--provider",
			"graph",
			"--job",
			"ghost",
			"--unconditional",
			"--changed-files",
			"docs/b.md",
		],
	)
	.unwrap();
	run_mc(
		root,
		&[
			"ci",
			"verify",
			"--job",
			"ghost",
			"--conclusion",
			"success",
			"--duration-seconds",
			"0",
			"--changed-files",
			"docs/b.md",
		],
	)
	.unwrap();

	let output = run_mc(root, &["ci", "analytics", "--format", "json"]).unwrap();
	let report: CiAnalyticsReport = serde_json::from_str(&output).unwrap();

	assert_eq!(report.false_positives, 1);
	assert!((report.wasted_vm_minutes - 5.0).abs() < 1e-9);
	assert!((report.estimated_vm_minutes_saved - 5.0).abs() < 1e-9);

	let history_text = run_mc(root, &["ci", "history", "--format", "text"]).unwrap();

	assert!(
		history_text.contains("decision ["),
		"unexpected: {history_text}"
	);
	assert!(
		history_text.contains("verification ["),
		"unexpected: {history_text}"
	);

	let unlimited = run_mc(
		root,
		&["ci", "history", "--limit", "10", "--format", "json"],
	)
	.unwrap();
	let unlimited_report: CiHistoryReport = serde_json::from_str(&unlimited).unwrap();

	assert_eq!(unlimited_report.decisions.len(), 2);
	assert_eq!(unlimited_report.verifications.len(), 1);
}

#[test]
fn verify_reports_memory_store_load_failures() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/verify-errors.git");
	fs::create_dir_all(root.join(".monochange")).unwrap();
	fs::write(root.join(CI_MEMORY_RELATIVE_PATH), "{ not json").unwrap();

	let error = run_mc(
		root,
		&[
			"ci",
			"verify",
			"--job",
			"broken-store",
			"--conclusion",
			"success",
		],
	)
	.unwrap_err();

	assert!(
		error.to_string().contains("failed to parse"),
		"unexpected error: {error}"
	);
}

#[test]
fn render_analytics_helpers_cover_empty_sections_and_missing_learned_rules() {
	let report = CiAnalyticsReport {
		repo_id: "github.com/acme/render".to_string(),
		repo_hash: "hash".to_string(),
		authenticated: true,
		total_decisions: 0,
		run_decisions: 0,
		skip_decisions: 0,
		unconditional_decisions: 0,
		skip_rate: 0.0,
		estimated_vm_minutes_saved: 0.0,
		wasted_vm_minutes: 0.0,
		true_positives: 0,
		true_negatives: 0,
		false_positives: 0,
		false_negatives: 1,
		precision: 1.0,
		recall: 1.0,
		false_negative_rate: 0.0,
		learned_rules_count: 0,
		false_negative_incidents: vec![CiFalseNegativeIncident {
			id: "fn-1".to_string(),
			job: "job".to_string(),
			commit: None,
			pr: None,
			changed_files: vec!["src/lib.rs".to_string()],
			reason: "reason".to_string(),
			learned_rule: None,
			timestamp: "2026-01-01T00:00:00Z".to_string(),
		}],
		false_positive_incidents: Vec::new(),
		tuning_recommendations: Vec::new(),
		slow_jobs: Vec::new(),
	};

	let markdown = render::render_analytics_report(&report, "markdown").unwrap();

	assert!(
		markdown.contains("learned rule: `none`"),
		"unexpected: {markdown}"
	);
	assert!(
		markdown.contains("(`false_positive`)\nNone."),
		"unexpected: {markdown}"
	);
	assert!(
		markdown.contains("## Slow Job Red Flags\nNone."),
		"unexpected: {markdown}"
	);

	let text = render::render_analytics_report(&report, "text").unwrap();

	assert!(text.contains("learned_rule=none"), "unexpected: {text}");
	assert!(
		text.contains("false_positives (ran `y`, but shouldn't have):\n  none"),
		"unexpected: {text}"
	);
	assert!(text.contains("slow_jobs:\n  none"), "unexpected: {text}");

	let mut empty = report.clone();

	empty.false_negative_incidents.clear();

	let empty_markdown = render::render_analytics_report(&empty, "markdown").unwrap();

	assert!(
		empty_markdown.contains("(`false_negative`)\nNone."),
		"unexpected: {empty_markdown}"
	);

	let empty_text = render::render_analytics_report(&empty, "text").unwrap();

	assert!(
		empty_text.contains("false_negatives (didn't run `n`, but should have):\n  none"),
		"unexpected: {empty_text}"
	);
}

#[test]
fn decision_provider_parsing_labels_and_classifications_cover_every_variant() {
	for (name, provider) in [
		("opencode-go", CiDecisionProvider::OpencodeGo),
		("openrouter", CiDecisionProvider::Openrouter),
		("cloudflare", CiDecisionProvider::Cloudflare),
		("jev", CiDecisionProvider::Jev),
		("openai-compatible", CiDecisionProvider::OpenaiCompatible),
		("graph", CiDecisionProvider::Graph),
	] {
		assert_eq!(CiDecisionProvider::parse(name).unwrap(), provider);
		assert_eq!(provider.as_str(), name);
	}

	assert_eq!(
		CiDecisionProvider::parse(" graph ").unwrap(),
		CiDecisionProvider::Graph
	);

	let unsupported = CiDecisionProvider::parse("bogus").unwrap_err();

	assert!(
		unsupported
			.to_string()
			.contains("unsupported CI decision provider `bogus`"),
		"unexpected error: {unsupported}"
	);

	for (classification, label) in [
		(CiVerificationClassification::TruePositive, "true_positive"),
		(CiVerificationClassification::TrueNegative, "true_negative"),
		(
			CiVerificationClassification::FalsePositive,
			"false_positive",
		),
		(
			CiVerificationClassification::FalseNegative,
			"false_negative",
		),
	] {
		assert_eq!(classification.as_str(), label);
	}
}
