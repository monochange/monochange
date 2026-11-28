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

/// Write a two-crate cargo workspace plus `[ci]` configuration.
fn write_ci_workspace(root: &Path, ci_toml: &str) {
	write_graph_workspace(root);

	let existing = fs::read_to_string(root.join("monochange.toml")).unwrap();

	fs::write(
		root.join("monochange.toml"),
		format!("{existing}\n{ci_toml}\n"),
	)
	.unwrap();
}

#[test]
fn ci_decide_evaluates_unconditional_overrides_and_max_skips_on_main() {
	let repo = tempdir().unwrap();

	init_git_repo_with_origin(repo.path(), "https://github.com/acme/monorepo.git");
	write_ci_workspace(
		repo.path(),
		"[ci]\nmax_skips_on_main = 2\n\n[ci.jobs.rust-tests]\npackages = [\"app\"]\n\n[ci.jobs.main-gate-job]\npackages = [\"app\"]\n",
	);

	// 1. Docs-only change on PR should skip (`n`) for rust-tests
	let out_skip = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
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

	// 7. Consecutive skips on `main` reaching `[ci] max_skips_on_main` (set to 2)
	for expected_skips in 1..=2 {
		let out_main_skip = run_mc(
			repo.path(),
			&[
				"ci",
				"decide",
				"--job",
				"main-gate-job",
				"--branch",
				"main",
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
			"--job",
			"main-gate-job",
			"--branch",
			"main",
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
	write_ci_workspace(
		repo.path(),
		"[ci.jobs.unit-tests]\npackages = [\"app\"]\n\n[ci.jobs.lint-job]\npackages = [\"app\"]\n",
	);

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
					"content": "{\"api-tests\":{\"run\":false,\"reason\":\"opencode-go determined job is unaffected\"}}"
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
			"api-tests": {
				"run": true,
				"reason": "jev webhook requested execution"
			}
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
			"crates/app/src/lib.rs",
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
			"crates/app/src/lib.rs",
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
	write_ci_workspace(repo.path(), "[ci.jobs.api-tests]\npackages = [\"app\"]\n");

	// The graph wants to run (`app` root is affected) and `ai_can_skip`
	// defaults to false, so the provider skip must be ignored.
	let opencode_report = run_opencode_go_decide(repo.path(), &server);

	opencode_mock.assert();
	assert_eq!(opencode_mock.calls(), 1, "one request per decide run");
	assert_eq!(opencode_report.decision, "y");
	assert_eq!(opencode_report.provider, "opencode-go");
	assert_eq!(opencode_report.model, DEFAULT_OPENCODE_GO_MODEL);
	assert!(
		opencode_report
			.jobs
			.first()
			.is_some_and(|job| job.reason.contains("ai_can_skip = false")),
		"unexpected reason: {}",
		opencode_report.reason
	);

	let jev_output = run_jev_decide(repo.path(), &server);

	jev_mock.assert();
	assert!(jev_output.contains("decisions={\"api-tests\":true}"));
	assert!(jev_output.contains("affected-packages=[\"app\"]"));
	assert!(jev_output.contains("run-all=true"));
}

#[test]
fn ci_decide_falls_back_to_the_graph_when_the_provider_response_omits_a_job() {
	let server = MockServer::start();
	let chat_mock = server.mock(|when, then| {
		when.method(POST).path("/v1/chat/completions");
		then.status(HTTP_OK).json_body(json!({
			"choices": [{
				"message": {
					"content": "{\"unit\":{\"run\":false,\"reason\":\"provider skipped unit\"}}"
				}
			}]
		}));
	});
	let repo = tempdir().unwrap();

	init_git_repo_with_origin(repo.path(), "https://github.com/acme/omitted-job.git");
	write_ci_workspace(
		repo.path(),
		"[ci]\nai_can_skip = true\n\n[ci.jobs.unit]\npackages = [\"app\"]\n\n[ci.jobs.e2e]\npackages = [\"core\"]\n",
	);

	// `crates/core` is owned by the `e2e` root, so its graph verdict is `run`
	// even though the provider response never mentions the job.
	let output = run_mc(
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
			"--changed-files",
			"crates/core/src/lib.rs",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();

	chat_mock.assert();

	let e2e = report
		.jobs
		.iter()
		.find(|job| job.job == "e2e")
		.unwrap_or_else(|| panic!("e2e job missing"));

	assert!(
		e2e.reason.contains("provider response omitted this job"),
		"unexpected reason: {}",
		e2e.reason
	);
	assert!(e2e.run, "the graph verdict must stand: {}", e2e.reason);
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

/// Copy the committed gatekeeper fixture workspace into a temp directory.
///
/// Tests mutate the copy (adding crates, letting `cargo metadata` write a
/// `Cargo.lock`) so the committed fixture stays pristine.
fn gatekeeper_fixture() -> tempfile::TempDir {
	let dir = tempdir().unwrap();
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ci/gatekeeper");

	monochange_test_helpers::copy_directory(&fixture, dir.path());

	dir
}

#[test]
fn ci_decide_runs_always_jobs_and_unowned_changes_and_unconfigured_jobs() {
	let repo = gatekeeper_fixture();

	// A change inside a package: `test-windows` runs (root glob), `lint` runs
	// (`always = true`), `docs` skips.
	let output = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--changed-files",
			"crates/leptoskit_plugin_windowkit/src/lib.rs",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();

	assert_eq!(report.jobs.len(), 4, "every configured job is evaluated");
	assert!(
		report
			.jobs
			.iter()
			.any(|job| job.job == "test-windows" && job.run)
	);
	let lint = report
		.jobs
		.iter()
		.find(|job| job.job == "lint")
		.unwrap_or_else(|| panic!("lint job missing"));
	assert!(lint.run);
	assert_eq!(lint.reason, "`always = true` in `[ci.jobs]`");
	assert!(report.jobs.iter().any(|job| job.job == "docs" && !job.run));

	// An unowned change runs every configured job.
	let output = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--changed-files",
			"scripts/deploy.sh",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();

	assert!(
		report.jobs.iter().all(|job| job.run),
		"unowned changes run every job"
	);
	assert!(
		report
			.jobs
			.iter()
			.filter(|job| job.job != "lint")
			.all(|job| {
				job.reason
					.contains("unowned changed file `scripts/deploy.sh`")
			})
	);

	// An unconfigured `--job` id runs with a warning and the no-rule reason.
	let output = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--job",
			"brand-new-job",
			"--changed-files",
			"crates/gallery/src/lib.rs",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();

	assert_eq!(report.jobs.len(), 1);
	assert!(report.jobs[0].run);
	assert_eq!(report.jobs[0].reason, "no ci rule for `brand-new-job`");

	// Neither `--job` nor configured jobs is a configuration error.
	let empty = tempdir().unwrap();

	support::init_git_repo_with_origin(empty.path(), "https://github.com/acme/empty-ci.git");
	let error = run_mc(empty.path(), &["ci", "decide"]).unwrap_err();

	assert!(
		error.to_string().contains("no `[ci.jobs]` configured"),
		"unexpected error: {error}"
	);
}

#[test]
fn ci_check_reports_every_configuration_problem_class() {
	let repo = gatekeeper_fixture();

	// Track files so the path-glob check can see them.
	support::init_git_repo_with_origin(repo.path(), "https://github.com/acme/check.git");
	fs::write(
		repo.path().join(".monochange-tmp-marker"),
		"marker so git add has something",
	)
	.unwrap();
	run_git(repo.path(), &["add", "."]);

	// Inject every problem class into the configuration and workflow wiring.
	fs::write(
		repo.path().join("monochange.toml"),
		"[ci.jobs.test-windows]\npackages = [\"leptoskit_windows\", \"ghost_crate\", \"missing_plugin_*\", { name = \"ghost_table\" }, { name = \"gone_plugin_*\", all_features = true }]\npaths = [\"docs/**\", \"vanished/**/*.snap\"]\n\n[ci.jobs.Broken_Name]\npackages = [\"leptoskit_windows\"]\n\n[ci.jobs.empty-job]\n",
	)
	.unwrap();
	fs::create_dir_all(repo.path().join(".github/workflows")).unwrap();
	fs::write(
		repo.path().join(".github/workflows/ci.yml"),
		"jobs:\n  gate:\n    outputs:\n      decisions: ${{ steps.gate.outputs.decisions }}\n      run-all: ${{ steps.gate.outputs.run-all }}\n  a:\n    needs: gate\n    if: needs.gate.result != 'success' || needs.gate.outputs.run-all == 'true' || fromJSON(needs.gate.outputs.decisions)['ghost-job']\n  b:\n    needs: gate\n    if: needs.gate.result != 'success' || needs.gate.outputs.run-all == 'true' || fromJSON(needs.gate.outputs.decisions).another-ghost\n",
	)
	.unwrap();

	let error = run_mc(
		repo.path(),
		&["ci", "check", "--workflow", ".github/workflows/ci.yml"],
	)
	.unwrap_err();
	let problems = error.to_string();

	assert!(
		problems.contains("ci check found"),
		"unexpected output: {problems}"
	);
	assert!(
		problems.contains("names unknown root package `ghost_crate`"),
		"missing unknown-package problem: {problems}"
	);
	assert!(
		problems.contains("package glob `missing_plugin_*` matches zero workspace packages"),
		"missing zero-match glob problem: {problems}"
	);
	assert!(
		problems.contains("names unknown root package `ghost_table`"),
		"missing unknown-table-package problem: {problems}"
	);
	assert!(
		problems.contains("package glob `gone_plugin_*` matches zero workspace packages"),
		"missing zero-match table-glob problem: {problems}"
	);
	assert!(
		problems.contains("path glob `vanished/**/*.snap` matches zero tracked files"),
		"missing zero-match path problem: {problems}"
	);
	assert!(
		problems.contains("`empty-job` sets none of `packages`, `paths`, or `always`"),
		"missing empty-job problem: {problems}"
	);
	assert!(
		problems.contains("job id `Broken_Name` is not kebab-case"),
		"missing job-id problem: {problems}"
	);
	assert!(
		problems.contains("workflow references job `ghost-job`"),
		"missing bracket-reference problem: {problems}"
	);
	assert!(
		problems.contains("workflow references job `another-ghost`"),
		"missing dot-reference problem: {problems}"
	);
	assert!(
		problems.contains("configured job `empty-job` is never referenced"),
		"missing never-referenced problem: {problems}"
	);

	// The pristine fixture configuration passes, including its workflow wiring.
	let repo_ok = gatekeeper_fixture();

	support::init_git_repo_with_origin(repo_ok.path(), "https://github.com/acme/check-ok.git");
	run_git(repo_ok.path(), &["add", "."]);

	let output = run_mc(
		repo_ok.path(),
		&[
			"ci",
			"check",
			"--workflow",
			".github/workflows/ci.yml",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: serde_json::Value = serde_json::from_str(&output).unwrap();

	assert_eq!(report["ok"], serde_json::json!(true));
}

#[test]
fn ci_check_reports_decisions_references_missing_fail_safe_guards() {
	let repo = gatekeeper_fixture();

	support::init_git_repo_with_origin(repo.path(), "https://github.com/acme/check-guards.git");
	run_git(repo.path(), &["add", "."]);

	fs::create_dir_all(repo.path().join(".github/workflows")).unwrap();
	fs::write(
		repo.path().join(".github/workflows/guards.yml"),
		"jobs:\n  gate:\n    outputs:\n      decisions: ${{ steps.gate.outputs.decisions }}\n      run-all: ${{ steps.gate.outputs.run-all }}\n  safe:\n    needs: gate\n    if: needs.gate.result != 'success' || needs.gate.outputs.run-all == 'true' || fromJSON(needs.gate.outputs.decisions)['test-windows'] || fromJSON(needs.gate.outputs.decisions)['lint']\n  unsafe:\n    needs: gate\n    if: fromJSON(needs.gate.outputs.decisions)['facade-nightly']\n  half-safe:\n    needs: gate\n    if: needs.gate.outputs.run-all == 'true' || fromJSON(needs.gate.outputs.decisions)['docs']\n",
	)
	.unwrap();

	let error = run_mc(
		repo.path(),
		&["ci", "check", "--workflow", ".github/workflows/guards.yml"],
	)
	.unwrap_err();
	let problems = error.to_string();

	assert!(
		problems.contains(
			"workflow `.github/workflows/guards.yml` line 11 references `needs.gate.outputs.decisions` without `needs.gate.outputs.run-all == 'true'` and `needs.gate.result != 'success'`"
		),
		"missing both-guards problem: {problems}"
	);
	assert!(
		problems.contains(
			"workflow `.github/workflows/guards.yml` line 14 references `needs.gate.outputs.decisions` without `needs.gate.result != 'success'`"
		),
		"missing single-guard problem: {problems}"
	);
	assert!(
		!problems.contains("guards.yml` line 8 references"),
		"guarded reference must pass: {problems}"
	);
}

#[test]
fn ci_check_renders_text_success_and_json_problems() {
	let repo = gatekeeper_fixture();

	support::init_git_repo_with_origin(repo.path(), "https://github.com/acme/check-formats.git");
	run_git(repo.path(), &["add", "."]);

	// Without `--workflow` no workflow scan runs; text is the default format.
	let text = run_mc(repo.path(), &["ci", "check"]).unwrap();

	assert!(
		text.contains("ci configuration valid: 4 job(s) checked (graph: cargo metadata)"),
		"unexpected text output: {text}"
	);

	// An unreadable workflow becomes a problem; JSON rendering carries it.
	let error = run_mc(
		repo.path(),
		&[
			"ci",
			"check",
			"--workflow",
			".github/workflows/missing.yml",
			"--format",
			"json",
		],
	)
	.unwrap_err();
	let problems = error.to_string();

	assert!(
		problems.contains("could not read workflow"),
		"missing problem: {problems}"
	);
	assert!(
		problems.contains("\"ok\":false"),
		"expected JSON problems payload: {problems}"
	);
}

/// Run a git command inside `root`.
fn run_git(root: &Path, args: &[&str]) {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap();

	assert!(
		output.status.success(),
		"git {:?} failed: {}",
		args,
		String::from_utf8_lossy(&output.stderr)
	);
}

/// Overwrite the fixture's `[ci]` configuration with one `facade` root job.
fn write_facade_job(root: &Path, packages_toml: &str) -> monochange_config::CiSettings {
	fs::write(
		root.join("monochange.toml"),
		format!("[ci.jobs.facade-job]\npackages = [{packages_toml}]\n"),
	)
	.unwrap();

	monochange_config::load_ci_settings(root).unwrap()
}

/// Evaluate one changed file against the `facade-job` root entry.
fn facade_verdict(
	root: &Path,
	settings: &monochange_config::CiSettings,
	file: &str,
) -> graph::GraphVerdict {
	let context = graph::evaluate_graph_context(root, &[file.to_string()], settings).unwrap();

	graph::evaluate_graph_decision_for_job("facade-job", settings.jobs.get("facade-job"), &context)
}

/// Assert a table of changed-file expectations against the facade job.
fn assert_facade_closure(
	root: &Path,
	settings: &monochange_config::CiSettings,
	expectations: &[(&str, bool)],
) {
	for (file, expected) in expectations {
		let verdict = facade_verdict(root, settings, file);

		assert_eq!(verdict.run, *expected, "{file}: {}", verdict.reason);
	}
}

#[test]
fn feature_aware_roots_follow_only_the_edges_the_selection_builds() {
	let repo = gatekeeper_fixture();
	let root = repo.path();

	// `default_features = false` builds no optional dependency: `opt_dep`
	// (implicit optional feature), `opt_hidden` (`dep:`), and their leaves
	// stay out. The root's normal edge `mid` and root dev-edge `testkit`
	// still build, and `testkit` re-enables `mid`'s defaults so `shared`
	// joins through feature unification even though `facade`'s own `mid`
	// entry disables them.
	let no_defaults = write_facade_job(root, r#"{ name = "facade", default_features = false }"#);

	assert_facade_closure(
		root,
		&no_defaults,
		&[
			("crates/mid/src/lib.rs", true),
			("crates/testkit/src/lib.rs", true),
			("crates/shared/src/lib.rs", true),
			("crates/opt_dep/src/lib.rs", false),
			("crates/opt_hidden/src/lib.rs", false),
			("crates/hidden_leaf/src/lib.rs", false),
			("crates/shared_extra/src/lib.rs", false),
			("crates/opt_boosted/src/lib.rs", false),
			("crates/mid_dev_only/src/lib.rs", false),
		],
	);

	// `features = ["nightly"]` activates `dep:opt_hidden` (whose dependency
	// entry enables `opt_hidden`'s `extra_kit`, pulling `hidden_leaf`) and
	// `mid/extra` (`shared_extra`), while `opt_dep?/boosted` stays dormant
	// because nothing else activates `opt_dep`. `mid_dev_only` is a dev edge
	// of a transitive dependency and never builds.
	let nightly = write_facade_job(root, r#"{ name = "facade", features = ["nightly"] }"#);

	assert_facade_closure(
		root,
		&nightly,
		&[
			("crates/opt_hidden/src/lib.rs", true),
			("crates/hidden_leaf/src/lib.rs", true),
			("crates/shared_extra/src/lib.rs", true),
			("crates/shared/src/lib.rs", true),
			("crates/opt_dep/src/lib.rs", false),
			("crates/opt_boosted/src/lib.rs", false),
			("crates/mid_dev_only/src/lib.rs", false),
		],
	);

	// Adding the implicit `opt_dep` feature activates the optional dep and
	// arms `opt_dep?/boosted`, whose `boosted` pulls `opt_boosted`.
	let nightly_with_opt_dep = write_facade_job(
		root,
		r#"{ name = "facade", features = ["nightly", "opt_dep"] }"#,
	);

	assert_facade_closure(
		root,
		&nightly_with_opt_dep,
		&[
			("crates/opt_dep/src/lib.rs", true),
			("crates/opt_boosted/src/lib.rs", true),
			("crates/mid_dev_only/src/lib.rs", false),
		],
	);

	// A package without a `default` feature starts its selection empty.
	let empty_selection = write_facade_job(root, r#"{ name = "opt_dep", features = [] }"#);

	assert_facade_closure(
		root,
		&empty_selection,
		&[("crates/opt_boosted/src/lib.rs", false)],
	);

	// `facade` lists `opt_dep` twice: once plainly and once under a target
	// table that enables `boosted`. Activating `opt_dep` (without `nightly`,
	// so `opt_dep?/boosted` stays dormant) must follow both entries, so
	// `opt_boosted` builds through the target entry's feature list.
	let duplicate_entries =
		write_facade_job(root, r#"{ name = "facade", features = ["opt_dep"] }"#);

	assert_facade_closure(
		root,
		&duplicate_entries,
		&[
			("crates/opt_dep/src/lib.rs", true),
			("crates/opt_boosted/src/lib.rs", true),
		],
	);
}

#[test]
fn ci_decide_reports_feature_fallback_warnings_in_job_reasons() {
	let repo = gatekeeper_fixture();

	fs::write(
		repo.path().join("monochange.toml"),
		"[ci.jobs.facade-job]\npackages = [{ name = \"facade\", features = [\"bogus\"] }]\n",
	)
	.unwrap();

	let output = run_mc(
		repo.path(),
		&[
			"ci",
			"decide",
			"--job",
			"facade-job",
			"--changed-files",
			"crates/opt_dep/src/lib.rs",
			"--format",
			"json",
		],
	)
	.unwrap();
	let report: CiDecideReport = serde_json::from_str(&output).unwrap();

	assert_eq!(report.jobs.len(), 1);
	assert!(report.jobs[0].run);
	assert!(
		report.jobs[0]
			.reason
			.contains("feature `bogus` is not defined by root package `facade`"),
		"missing fallback warning: {}",
		report.jobs[0].reason
	);
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
	let parentless = PackageRecord::new(
		Ecosystem::Cargo,
		"parentless",
		std::path::PathBuf::from("/"),
		root.to_path_buf(),
		None,
		PublishState::Public,
	);

	assert_eq!(graph::relative_package_dir(&inside, root), "crates/core");
	assert_eq!(graph::relative_package_dir(&at_root, root), "");
	assert_eq!(graph::relative_package_dir(&outside, root), "../outside");
	assert_eq!(graph::relative_package_dir(&parentless, root), "");
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

	let both = graph::detect_changed_files_from_git(root, Some("HEAD~1"), Some("HEAD")).unwrap();

	assert_eq!(both, vec!["second.txt".to_string()]);

	let base_only = graph::detect_changed_files_from_git(root, Some("HEAD~1"), None).unwrap();

	assert_eq!(base_only, vec!["second.txt".to_string()]);

	let head_only = graph::detect_changed_files_from_git(root, None, Some("HEAD")).unwrap();

	assert!(head_only.is_empty());
	assert_eq!(
		graph::detect_changed_files_from_git(root, None, None).unwrap(),
		Vec::<String>::new()
	);

	assert_eq!(graph::diff_specification(Some("a"), Some("b")), "a...b");
	assert_eq!(graph::diff_specification(Some("a"), None), "a");
	assert_eq!(graph::diff_specification(None, Some("b")), "b");
	assert_eq!(graph::diff_specification(None, None), "HEAD");
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
fn decide_unconditional_ref_labels_and_configured_paths_force_runs() {
	let dir = tempdir().unwrap();

	init_git_repo_with_origin(dir.path(), "https://github.com/acme/unconditional.git");
	write_ci_workspace(
		dir.path(),
		"[ci]\nunconditional_paths = [\"custom/*\"]\n\n[ci.jobs.rust-tests]\npackages = [\"app\"]\n",
	);

	let output = run_mc(
		dir.path(),
		&[
			"ci",
			"decide",
			"--job",
			"release-job",
			"--ref",
			"refs/tags/v1.0.0",
			"--label",
			"[ci:full]",
			"--changed-files",
			"custom/extra.txt,docs/guide.md",
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
	assert!(
		reasons.contains("ref `refs/tags/v1.0.0` matches unconditional branch/ref pattern"),
		"unexpected reasons: {reasons}"
	);
	assert!(reasons.contains("label `[ci:full]` forces unconditional CI"));
	assert!(
		reasons.contains(
			"changed file `custom/extra.txt` matches unconditional path pattern `custom/*`"
		),
		"unexpected reasons: {reasons}"
	);

	let text = run_mc(
		dir.path(),
		&[
			"ci",
			"decide",
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
fn provider_payloads_cover_direct_nested_fenced_and_error_responses() {
	let run =
		provider::parse_provider_decisions(r#"{"job-a": {"run": true, "reason": "affected"}}"#)
			.unwrap();

	assert_eq!(run.get("job-a").map(|decision| decision.run), Some(true));
	assert_eq!(
		run.get("job-a").map(|decision| decision.reason.as_str()),
		Some("affected")
	);

	let default_reason =
		provider::parse_provider_decisions(r#"{"job-a": {"run": false}}"#).unwrap();

	assert_eq!(
		default_reason
			.get("job-a")
			.map(|decision| decision.reason.as_str()),
		Some("provider decided to skip job")
	);

	let decision_alias =
		provider::parse_provider_decisions(r#"{"job-a": {"decision": "y"}}"#).unwrap();

	assert_eq!(
		decision_alias.get("job-a").map(|decision| decision.run),
		Some(true)
	);

	let nested = provider::parse_provider_decisions(
		r#"{"choices":[{"message":{"content":"{\"job-a\":{\"run\":false,\"reason\":\"nested\"}}"}}]}"#,
	)
	.unwrap();

	assert_eq!(
		nested.get("job-a").map(|decision| decision.run),
		Some(false)
	);
	assert_eq!(
		nested.get("job-a").map(|decision| decision.reason.as_str()),
		Some("nested")
	);

	let fenced =
		provider::parse_provider_decisions("```json\n{\"job-a\": {\"run\": true}}\n```").unwrap();

	assert_eq!(fenced.get("job-a").map(|decision| decision.run), Some(true));

	let missing =
		provider::parse_provider_decisions(r#"{"job-a": {"reason": "no verdict"}}"#).unwrap_err();

	assert!(
		missing.contains("did not include `run`"),
		"unexpected error: {missing}"
	);

	let not_object = provider::parse_provider_decisions(r#"["job-a"]"#).unwrap_err();

	assert!(
		not_object.contains("was not a JSON object"),
		"unexpected error: {not_object}"
	);

	let invalid = provider::parse_provider_decisions("not json").unwrap_err();

	assert!(
		invalid.contains("invalid JSON response"),
		"unexpected error: {invalid}"
	);
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
			("JEV_API_URL", None),
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
		provider::resolve_chat_endpoint(CiDecisionProvider::OpencodeGo, &with_key).unwrap();

	assert_eq!(opencode.url, DEFAULT_OPENCODE_GO_API_URL);
	assert_eq!(opencode.api_key, "key");
	assert_eq!(opencode.default_model, DEFAULT_OPENCODE_GO_MODEL);

	let cloudflare =
		provider::resolve_chat_endpoint(CiDecisionProvider::Cloudflare, &with_key).unwrap();

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
		provider::resolve_chat_endpoint(CiDecisionProvider::Openrouter, &custom)
			.unwrap()
			.url,
		"http://localhost:9/v1/chat/completions"
	);

	let compatible =
		provider::resolve_chat_endpoint(CiDecisionProvider::OpenaiCompatible, &custom).unwrap();

	assert_eq!(compatible.url, "http://localhost:9/v1/chat/completions");
	assert_eq!(compatible.default_model, DEFAULT_OPENCODE_GO_MODEL);

	let missing_key = provider::ProviderOverrides {
		model: None,
		api_url: Some("http://localhost:9"),
		api_key: None,
		account_id: None,
	};

	assert!(provider::resolve_chat_endpoint(CiDecisionProvider::OpencodeGo, &missing_key).is_err());
	assert!(provider::resolve_chat_endpoint(CiDecisionProvider::Openrouter, &missing_key).is_err());
	assert!(provider::resolve_chat_endpoint(CiDecisionProvider::Cloudflare, &missing_key).is_err());

	let no_url = provider::ProviderOverrides {
		model: None,
		api_url: None,
		api_key: Some("key"),
		account_id: None,
	};

	assert!(
		provider::resolve_chat_endpoint(CiDecisionProvider::OpenaiCompatible, &no_url).is_err()
	);
	assert!(
		provider::resolve_provider_url(CiDecisionProvider::Jev, &no_url)
			.unwrap_err()
			.contains("missing --api-url or JEV_API_URL for jev")
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
	fs::write(
		root.join("monochange.toml"),
		"[ci.jobs.prose-job]\npaths = [\"rare/**\"]\n\n[ci.jobs.forced-job]\npaths = [\"rare/**\"]\n\n[ci.jobs.text-job]\npaths = [\"rare/**\"]\n",
	)
	.unwrap();

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

	// An unconfigured job always runs, so a passing verification of it with
	// no files is a useful run, not a safe skip.
	let orphan = verify_record(root, &["--job", "orphan-job", "--conclusion", "success"]);

	assert!(orphan.changed_files.is_empty());
	assert_eq!(orphan.prior_decision, "y");
	assert_eq!(
		orphan.classification,
		CiVerificationClassification::TruePositive
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
fn verify_rejects_skipped_or_cancelled_conclusions_without_recording() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/verify-no-evidence.git");
	run_mc(root, &["ci", "auth", "init"]).unwrap();
	fs::write(
		root.join("monochange.toml"),
		"[ci.jobs.docs]\npaths = [\"rare/**\"]\n",
	)
	.unwrap();

	for conclusion in ["skipped", "cancelled"] {
		let error = run_mc(
			root,
			&[
				"ci",
				"verify",
				"--job",
				"docs",
				"--conclusion",
				conclusion,
				"--changed-files",
				"docs/guide.md",
			],
		)
		.unwrap_err();

		assert!(
			error
				.to_string()
				.contains(&format!("job `docs` concluded `{conclusion}`")),
			"unexpected error for `{conclusion}`: {error}"
		);
	}

	// A refused outcome leaves no verification record behind.
	let history_output = run_mc(root, &["ci", "history", "--format", "json"]).unwrap();
	let history: CiHistoryReport = serde_json::from_str(&history_output).unwrap();

	assert!(history.verifications.is_empty());

	// `success` is still the one conclusion that verifies a skip as a true negative.
	let success = verify_record(
		root,
		&[
			"--job",
			"docs",
			"--conclusion",
			"success",
			"--changed-files",
			"docs/guide.md",
		],
	);

	assert_eq!(success.prior_decision, "n");
	assert_eq!(
		success.classification,
		CiVerificationClassification::TrueNegative
	);
}

#[test]
fn analytics_uses_fallback_minutes_for_unmeasured_durations() {
	let dir = tempdir().unwrap();
	let root = dir.path();

	init_git_repo_with_origin(root, "https://github.com/acme/analytics-fallback.git");
	run_mc(root, &["ci", "auth", "init"]).unwrap();
	fs::write(
		root.join("monochange.toml"),
		"[ci.jobs.unmeasured]\npackages = [\"never_a_package\"]\n",
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
