//! Hosted release commit and release request integration tests.
//!
//! The fixture workspace configures `CommitRelease` and `OpenReleaseRequest`
//! with the hosted backend. The tests point `MONOCHANGE_HOSTED_URL` at an
//! httpmock server that stands in for the monochange app and assert the CLI
//! sends the prepared release files and publishes the pull request through
//! the app instead of committing locally.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;

use httpmock::HttpMockRequest;
use httpmock::Method;
use httpmock::MockServer;
use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use monochange_test_helpers::git::git_output;
use rstest::rstest;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_hosted_fixture() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("hosted-release-commit/workspace"), root);
	git(root, &["init", "--initial-branch", "main"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

/// Run `monochange run release` in the fixture with the hosted app mocked.
fn run_hosted_release(root: &Path, server: &MockServer, dry_run: bool) -> std::process::Output {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_HOSTED_URL", server.base_url())
		.env("GITHUB_REPOSITORY", "acme/actions")
		.env("GITHUB_REF_NAME", "main")
		// The token path must not see a runner-provided OIDC context (the
		// coverage job grants `id-token: write`, so GitHub injects the
		// ACTIONS_ID_TOKEN_* variables into the test process environment).
		.env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
		.env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
		.env_remove("GITHUB_HEAD_REF")
		.env("MONOCHANGE_TOKEN", "monochange-token")
		.arg("run")
		.arg("release")
		.arg("--format")
		.arg("json");
	if dry_run {
		command.arg("--dry-run");
	}
	command
		.output()
		.unwrap_or_else(|error| panic!("run hosted release: {error}"))
}

fn mock_release_commit(server: &MockServer) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/api/release-commits")
			.header("Authorization", "Bearer monochange-token")
			.body_includes("\"owner\":\"acme\"")
			.body_includes("\"repository\":\"actions\"")
			.body_includes("\"files\":");
		then.status(200).json_body(serde_json::json!({
			"commit": "hosted-commit-sha",
			"verified": true,
			"status": "completed",
		}));
	})
}

fn mock_release_request(server: &MockServer) -> httpmock::Mock<'_> {
	mock_release_request_as(server, "monochange-token")
}

fn mock_release_request_as<'a>(server: &'a MockServer, bearer: &str) -> httpmock::Mock<'a> {
	server.mock(move |when, then| {
		when.method(Method::POST)
			.path("/api/release-requests")
			.header("Authorization", format!("Bearer {bearer}"))
			.body_includes("\"labels\":[\"release\"]")
			.body_includes("\"auto_merge\":true");
		then.status(200).json_body(serde_json::json!({
			"number": 42,
			"operation": "created",
			"url": "https://github.com/acme/actions/pull/42",
			"head_branch": "monochange/release/release",
		}));
	})
}

#[test]
fn hosted_release_commits_and_requests_publish_through_the_app() {
	let tempdir = setup_hosted_fixture();
	let root = tempdir.path();

	let server = MockServer::start();
	let commit_mock = mock_release_commit(&server);
	let request_mock = mock_release_request(&server);

	let output = run_hosted_release(root, &server, false);
	assert!(
		output.status.success(),
		"hosted release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	commit_mock.assert();
	request_mock.assert();

	// The hosted backend never commits locally; the working tree keeps the
	// prepared files and no release commit exists on the local branch.
	let status = git_output(root, &["status", "--short"]);
	assert!(
		status.to_lowercase().contains("changelog"),
		"prepared release files stay uncommitted in hosted mode: {status}"
	);
	assert_eq!(
		git_output(root, &["log", "-1", "--pretty=%s"]).trim(),
		"initial",
		"hosted mode must not create a local release commit"
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(
		stdout.contains("\"commit\": \"hosted-commit-sha\""),
		"report includes the hosted commit sha: {stdout}"
	);

	// mock_release_commit matched on owner, repository, and files, and
	// mock_release_request matched on the labels and auto-merge flag, so both
	// requests carried the prepared release payload.
}

#[test]
fn hosted_release_dry_run_skips_the_app() {
	let tempdir = setup_hosted_fixture();

	let server = MockServer::start();
	let commit_mock = mock_release_commit(&server);
	let request_mock = mock_release_request(&server);

	let output = run_hosted_release(tempdir.path(), &server, true);
	assert!(
		output.status.success(),
		"dry-run hosted release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	// A dry run reports the hosted flow without contacting the app.
	assert_eq!(commit_mock.calls(), 0);
	assert_eq!(request_mock.calls(), 0);
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(stdout.contains("dry_run"), "dry run is reported: {stdout}");
}

#[test]
fn hosted_release_reports_missing_github_repository() {
	let tempdir = setup_hosted_fixture();

	let server = MockServer::start();
	mock_release_commit(&server);
	mock_release_request(&server);

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(tempdir.path())
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_HOSTED_URL", server.base_url())
		.env("MONOCHANGE_TOKEN", "monochange-token")
		.env_remove("GITHUB_REPOSITORY")
		.env_remove("GITHUB_REF_NAME")
		.arg("run")
		.arg("release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run hosted release: {error}"));
	assert!(
		!output.status.success(),
		"missing GITHUB_REPOSITORY must fail the hosted backend"
	);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("GITHUB_REPOSITORY"),
		"error names the missing environment variable: {stderr}"
	);
}

#[test]
fn hosted_release_authenticates_with_github_actions_oidc() {
	let tempdir = setup_hosted_fixture();

	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/oidc")
			.query_param_includes("audience", "127.0.0.1")
			.header("Authorization", "Bearer runner-token");
		then.status(200)
			.json_body(serde_json::json!({ "value": "oidc-jwt" }));
	});
	let commit_mock = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/api/release-commits")
			.header("Authorization", "Bearer oidc-jwt");
		then.status(200).json_body(serde_json::json!({
			"commit": "hosted-commit-sha",
			"verified": true,
			"status": "completed",
		}));
	});
	let request_mock = mock_release_request_as(&server, "oidc-jwt");

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(tempdir.path())
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_HOSTED_URL", server.base_url())
		.env("GITHUB_REPOSITORY", "acme/actions")
		.env("GITHUB_REF_NAME", "main")
		.env_remove("MONOCHANGE_TOKEN")
		.env("ACTIONS_ID_TOKEN_REQUEST_URL", server.url("/oidc"))
		.env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "runner-token")
		.arg("run")
		.arg("release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run hosted release: {error}"));
	assert!(
		output.status.success(),
		"oidc-hosted release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	// The commit endpoint matched on the OIDC bearer token, so the CLI
	// exchanged the runner OIDC token for the audience derived from the
	// hosted URL host.
	commit_mock.assert();
	request_mock.assert();
}

#[test]
fn hosted_release_reports_missing_credentials() {
	let tempdir = setup_hosted_fixture();

	let server = MockServer::start();
	mock_release_commit(&server);
	mock_release_request(&server);

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(tempdir.path())
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_HOSTED_URL", server.base_url())
		.env("GITHUB_REPOSITORY", "acme/actions")
		.env("GITHUB_REF_NAME", "main")
		.env_remove("MONOCHANGE_TOKEN")
		.env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
		.env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
		.arg("run")
		.arg("release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run hosted release: {error}"));
	assert!(
		!output.status.success(),
		"missing credentials must fail the hosted backend"
	);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains("MONOCHANGE_TOKEN"),
		"error names the missing secret: {stderr}"
	);
}

/// Last JSON body a mock received, captured for snapshot assertions.
type CapturedBody = Arc<Mutex<Option<serde_json::Value>>>;

fn capture_body(
	captured: &CapturedBody,
) -> impl Fn(&HttpMockRequest) -> bool + Send + Sync + 'static {
	let captured = Arc::clone(captured);
	move |request| {
		let body = serde_json::from_slice(request.body().as_ref())
			.unwrap_or_else(|error| panic!("hosted request body is JSON: {error}"));
		*captured
			.lock()
			.unwrap_or_else(|error| panic!("capture lock poisoned: {error}")) = Some(body);
		true
	}
}

fn captured(body: &CapturedBody) -> serde_json::Value {
	body.lock()
		.unwrap_or_else(|error| panic!("capture lock poisoned: {error}"))
		.clone()
		.unwrap_or_else(|| panic!("the hosted endpoint was never called"))
}

/// Run one built-in step the way the hosted release action does on a push to
/// `main`: `GITHUB_HEAD_REF` is empty and `MONOCHANGE_HOSTED_URL` points at
/// an unreachable host, so only the command-line flags can reach the app.
fn run_hosted_step(root: &Path, server: &MockServer, step: &str, args: &[&str]) -> String {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.env("MONOCHANGE_HOSTED_URL", "http://127.0.0.1:9")
		.env("GITHUB_REPOSITORY", "acme/actions")
		.env("GITHUB_REF_NAME", "main")
		.env("GITHUB_HEAD_REF", "")
		.env_remove("GITHUB_RUN_ID")
		.env_remove("MONOCHANGE_TOKEN")
		.env("ACTIONS_ID_TOKEN_REQUEST_URL", server.url("/oidc"))
		.env("ACTIONS_ID_TOKEN_REQUEST_TOKEN", "runner-token")
		.arg("step")
		.arg(step)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run step {step}: {error}"));
	assert!(
		output.status.success(),
		"step {step} failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn hosted_step_commands_target_the_release_branch_with_command_line_settings() {
	let tempdir = setup_hosted_fixture();
	let root = tempdir.path();

	let server = MockServer::start();
	let oidc_mock = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/oidc")
			.query_param("audience", "flag-audience")
			.header("Authorization", "Bearer runner-token");
		then.status(200)
			.json_body(serde_json::json!({ "value": "oidc-jwt" }));
	});
	let commit_body = CapturedBody::default();
	let commit_mock = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/api/release-commits")
			.header("Authorization", "Bearer oidc-jwt")
			.is_true(capture_body(&commit_body));
		then.status(200).json_body(serde_json::json!({
			"commit": "hosted-commit-sha",
			"verified": true,
			"status": "completed",
		}));
	});
	let request_body = CapturedBody::default();
	let request_mock = server.mock(|when, then| {
		when.method(Method::POST)
			.path("/api/release-requests")
			.header("Authorization", "Bearer oidc-jwt")
			.is_true(capture_body(&request_body));
		then.status(200).json_body(serde_json::json!({
			"number": 42,
			"operation": "updated",
			"url": "https://github.com/acme/actions/pull/42",
			"head_branch": "monochange/release/release",
		}));
	});

	let hosted_url = server.base_url();
	let hosted_flags = [
		"--hosted-auth",
		"oidc",
		"--hosted-url",
		hosted_url.as_str(),
		"--oidc-audience",
		"flag-audience",
	];
	run_hosted_step(root, &server, "prepare-release", &[]);
	let commit_output = run_hosted_step(
		root,
		&server,
		"commit-release",
		&[
			&["--commit-backend", "hosted", "--format", "json"],
			hosted_flags.as_slice(),
		]
		.concat(),
	);
	let commit_output: serde_json::Value = serde_json::from_str(&commit_output)
		.unwrap_or_else(|error| panic!("parse commit-release json: {error}"));
	run_hosted_step(
		root,
		&server,
		"open-release-request",
		&[&["--backend", "hosted"], hosted_flags.as_slice()].concat(),
	);

	oidc_mock.assert_calls(2);
	commit_mock.assert();
	request_mock.assert();

	let commit = captured(&commit_body);
	let request = captured(&request_body);
	let hosted_requests = serde_json::json!({
		"commit": {
			"owner": commit["owner"],
			"repository": commit["repository"],
			"branch": commit["branch"],
			"base_branch": commit["base_branch"],
			"base_commit_is_head": commit["base_commit"].as_str()
				== Some(git_output(root, &["rev-parse", "HEAD"]).trim()),
			"files": commit["files"]
				.as_array()
				.map(|files| files.iter().map(|file| file["path"].clone()).collect::<Vec<_>>()),
		},
		"release_commit": {
			"commit": commit_output["release_commit"]["commit"],
			"verified": commit_output["release_commit"]["verified"],
			"status": commit_output["release_commit"]["status"],
		},
		"release_request": {
			"head_branch": request["request"]["head_branch"],
			"base_branch": request["request"]["base_branch"],
		},
	});
	assert_json_snapshot!(hosted_requests);
}

#[rstest]
#[case::commit_release("commit-invalid-auth")]
#[case::open_release_request("request-invalid-auth")]
fn hosted_steps_reject_unsupported_hosted_auth_inputs(#[case] command: &str) {
	let tempdir = setup_hosted_fixture();
	let server = MockServer::start();
	let commit_mock = mock_release_commit(&server);
	let request_mock = mock_release_request(&server);

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(tempdir.path())
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_HOSTED_URL", server.base_url())
		.env("GITHUB_REPOSITORY", "acme/actions")
		.env("MONOCHANGE_TOKEN", "monochange-token")
		.arg("run")
		.arg(command)
		.output()
		.unwrap_or_else(|error| panic!("run {command}: {error}"));

	assert!(!output.status.success(), "{command} must reject the input");
	assert_eq!(commit_mock.calls(), 0);
	assert_eq!(request_mock.calls(), 0);
	assert_snapshot!(
		format!("{command}_stderr"),
		String::from_utf8_lossy(&output.stderr)
	);
}
