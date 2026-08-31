//! Integration coverage for pub.dev trusted publishing token minting.
//!
//! These tests spawn the real monochange binary against a dart workspace, a
//! loopback mock for the pub.dev API, and a mock GitHub Actions OIDC token
//! endpoint. A stub `dart` executable records every invocation so the tests
//! can assert - end to end, with no dart SDK or network access - that dry runs
//! plan the publish without minting anything, that real publishes mint a fresh
//! pub.dev OIDC token right before `dart pub publish` and register it with
//! `dart pub token add https://pub.dev --env-var PUB_TOKEN`, that a broken
//! OIDC endpoint fails the publish fast instead of uploading with a stale
//! five-minute-old credential, and that runs without the GitHub Actions
//! endpoint keep the previous behavior.

#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]

use std::fs;
use std::io::Read;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use serde_json::Value;
use tempfile::TempDir;

const ACTIONS_ID_TOKEN_REQUEST_URL_ENV: &str = "ACTIONS_ID_TOKEN_REQUEST_URL";
const ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV: &str = "ACTIONS_ID_TOKEN_REQUEST_TOKEN";
const RUNNER_REQUEST_TOKEN: &str = "runner-request-secret";
const FRESH_ACTIONS_JWT: &str = "fresh-actions-oidc-jwt";
const OIDC_ROUTE: &str = "/token&audience=";
const PUB_DEV_ROUTE: &str = "GET /packages/";
const PUB_DEV_NOT_FOUND_RESPONSE: &str =
	"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const DART_TOKEN_ADD_ARGS: [&str; 6] = [
	"pub",
	"token",
	"add",
	"https://pub.dev",
	"--env-var",
	"PUB_TOKEN",
];
const DART_PUBLISH_ARGS: [&str; 3] = ["pub", "publish", "--force"];

fn http_ok_token_response(token: &str) -> String {
	let body = format!(r#"{{"count":1,"value":"{token}"}}"#);
	format!(
		"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
		body.len()
	)
}

fn http_server_error_response() -> String {
	String::from(
		"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
	)
}

fn fixture_workspace() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/cli-output/dart-trusted-publishing"),
		tempdir.path(),
	);
	tempdir
}

fn dart_stub_log_path(workspace: &TempDir) -> std::path::PathBuf {
	workspace.path().join("dart-stub.log")
}

/// Mock HTTP server for the pub.dev API and the runner OIDC token endpoint.
/// pub.dev API requests receive a 404 (the placeholder version does not exist
/// yet) and OIDC mint requests receive the configured response. Every request
/// head is captured for assertions.
struct MockServer {
	base_url: String,
	requests: Arc<Mutex<Vec<String>>>,
	stop: Arc<std::sync::atomic::AtomicBool>,
}

impl MockServer {
	/// Number of captured requests whose head contains `route`.
	fn request_count(&self, route: &str) -> usize {
		self.requests
			.lock()
			.unwrap()
			.iter()
			.filter(|head| head.contains(route))
			.count()
	}

	/// First captured request whose head contains the OIDC route.
	fn mint_head(&self) -> String {
		self.requests
			.lock()
			.unwrap()
			.iter()
			.find(|head| head.contains(OIDC_ROUTE))
			.cloned()
			.unwrap_or_else(|| panic!("expected a captured OIDC token request"))
	}
}

impl Drop for MockServer {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Relaxed);
	}
}

fn spawn_endpoint_server(oidc_response: Option<String>) -> MockServer {
	let oidc_response = oidc_response.unwrap_or_else(http_server_error_response);
	let listener = TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock endpoint server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("mock endpoint address: {error}"))
		.to_string();
	let requests = Arc::new(Mutex::new(Vec::new()));
	let captured = Arc::clone(&requests);
	let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
	let stop_for_thread = Arc::clone(&stop);
	std::thread::spawn(move || {
		let _ = listener.set_nonblocking(true);
		while !stop_for_thread.load(Ordering::Relaxed) {
			let Ok((mut stream, _)) = listener.accept() else {
				std::thread::sleep(Duration::from_millis(5));
				continue;
			};
			let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
			let mut head = Vec::new();
			let mut buffer = [0_u8; 4096];
			loop {
				match stream.read(&mut buffer) {
					Ok(0) | Err(_) => break,
					Ok(read) => {
						head.extend_from_slice(&buffer[..read]);
						if head.windows(4).any(|window| window == b"\r\n\r\n") {
							break;
						}
					}
				}
			}
			captured
				.lock()
				.unwrap()
				.push(String::from_utf8_lossy(&head).into_owned());
			let response = if String::from_utf8_lossy(&head).contains(OIDC_ROUTE) {
				oidc_response.clone()
			} else {
				PUB_DEV_NOT_FOUND_RESPONSE.to_string()
			};
			let _ = stream.write_all(response.as_bytes());
		}
	});
	MockServer {
		base_url: format!("http://{address}"),
		requests,
		stop,
	}
}
/// Spawn monochange with a clean ambient environment: GitHub Actions runner
/// variables and leftover pub credentials must not leak into the runs. The
/// pub.dev API endpoint points at the mock server, and when `actions_oidc` is
/// set the OIDC endpoint variables point at `<base>/token`.
fn base_command(root: &std::path::Path, pub_dev_base_url: &str) -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.current_dir(root);
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command.env("MONOCHANGE_NO_PROGRESS", "1");
	command.env("MONOCHANGE_PUB_DEV_API_URL", pub_dev_base_url);
	for env_var in [
		"GITHUB_ACTIONS",
		"GITHUB_REPOSITORY",
		"GITHUB_WORKFLOW",
		"GITHUB_WORKFLOW_REF",
		"GITHUB_ENVIRONMENT",
		"GITHUB_JOB",
		"GITHUB_RUN_ID",
		"GITHUB_REF_NAME",
		"MONOCHANGE_TRUSTED_PUBLISHING_ENVIRONMENT",
		"ACTIONS_ID_TOKEN_REQUEST_URL",
		"ACTIONS_ID_TOKEN_REQUEST_TOKEN",
		"PUB_TOKEN",
	] {
		command.env_remove(env_var);
	}
	command
}

fn placeholder_publish_command(
	root: &std::path::Path,
	base_url: &str,
	actions_oidc_base: Option<&str>,
	dry_run: bool,
) -> Command {
	let mut command = base_command(root, base_url);
	if let Some(oidc_base) = actions_oidc_base {
		command.env(
			ACTIONS_ID_TOKEN_REQUEST_URL_ENV,
			format!("{oidc_base}/token"),
		);
		command.env(ACTIONS_ID_TOKEN_REQUEST_TOKEN_ENV, RUNNER_REQUEST_TOKEN);
	}
	command.arg("step").arg("placeholder-publish");
	command.arg("--package").arg("dart_pkg");
	command.arg("--format").arg("json");
	if dry_run {
		command.arg("--dry-run");
	}
	command
}

fn run(mut command: Command, context: &str) -> std::process::Output {
	let output = command
		.output()
		.unwrap_or_else(|error| panic!("run {context}: {error}"));
	assert!(
		output.status.success(),
		"{context} should succeed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	output
}

fn package_publish_outcomes(stdout: &str, context: &str) -> Vec<Value> {
	let value: Value = serde_json::from_str(stdout)
		.unwrap_or_else(|error| panic!("parse publish report json: {error}"));
	value["package_publish"]["packages"]
		.as_array()
		.unwrap_or_else(|| {
			panic!("expected package_publish.packages array in the {context} report")
		})
		.clone()
}

/// A minimal `dart` replacement that records every invocation: the PUB_TOKEN
/// environment value and the command arguments. The tests reuse it to prove
/// what monochange actually spawned.
#[cfg(unix)]
const DART_STUB_SCRIPT: &str = r#"#!/bin/sh
{
  printf 'pub-token=%s\n' "${PUB_TOKEN-unset}"
  printf 'args|'
  for argument in "$@"; do printf '%s|' "$argument"; done
  printf '\n'
} >> "$DART_STUB_LOG"
exit 0
"#;

#[cfg(unix)]
#[derive(Debug)]
struct DartInvocation {
	pub_token: String,
	args: Vec<String>,
}

#[cfg(unix)]
fn install_dart_stub(workspace: &TempDir, log_path: &std::path::Path, command: &mut Command) {
	use std::os::unix::fs::PermissionsExt;

	let stub_dir = workspace.path().join(".stub-bin");
	fs::create_dir_all(&stub_dir).unwrap_or_else(|error| panic!("create stub bin dir: {error}"));
	let stub_path = stub_dir.join("dart");
	fs::write(&stub_path, DART_STUB_SCRIPT)
		.unwrap_or_else(|error| panic!("write dart stub: {error}"));
	fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755))
		.unwrap_or_else(|error| panic!("stub permissions: {error}"));
	fs::write(log_path, String::new())
		.unwrap_or_else(|error| panic!("create dart stub log: {error}"));

	let path = std::env::var("PATH").unwrap_or_default();
	command.env("PATH", format!("{}:{path}", stub_dir.display()));
	command.env("DART_STUB_LOG", log_path);
}

#[cfg(unix)]
fn read_dart_invocations(log_path: &std::path::Path) -> Vec<DartInvocation> {
	let contents =
		fs::read_to_string(log_path).unwrap_or_else(|error| panic!("read dart stub log: {error}"));
	let mut invocations = Vec::new();
	let mut pending: Option<DartInvocation> = None;
	for line in contents.lines() {
		if let Some(pub_token) = line.strip_prefix("pub-token=") {
			assert!(
				pending.is_none(),
				"consecutive pub-token lines without args in the dart stub log"
			);
			pending = Some(DartInvocation {
				pub_token: pub_token.to_string(),
				args: Vec::new(),
			});
		} else if let Some(args) = line.strip_prefix("args|") {
			let mut invocation = pending
				.take()
				.unwrap_or_else(|| panic!("dart stub args without a pub-token line"));
			invocation.args = args
				.split('|')
				.filter(|argument| !argument.is_empty())
				.map(str::to_string)
				.collect::<Vec<_>>();
			invocations.push(invocation);
		}
	}
	invocations
}

#[test]
fn dry_run_placeholder_publish_plans_dart_trusted_publishing_without_minting() {
	let server = spawn_endpoint_server(None);
	let workspace = fixture_workspace();
	let output = run(
		placeholder_publish_command(
			workspace.path(),
			&server.base_url,
			Some(&server.base_url),
			true,
		),
		"placeholder publish dry run",
	);
	let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
	let outcomes = package_publish_outcomes(&stdout, "placeholder publish dry run");

	// Dry runs plan the publish and never mint a token: the runtime mints right
	// before the real publication instead.
	assert_eq!(outcomes[0]["status"], "planned");
	assert_eq!(outcomes[0]["package"], "dart_pkg");
	assert!(server.request_count(PUB_DEV_ROUTE) >= 1);
	assert_eq!(server.request_count(OIDC_ROUTE), 0);
	assert_json_snapshot!(outcomes[0]);
}

#[test]
fn dart_placeholder_publish_fails_fast_when_the_oidc_token_cannot_be_minted() {
	let server = spawn_endpoint_server(None);
	let workspace = fixture_workspace();
	let output = placeholder_publish_command(
		workspace.path(),
		&server.base_url,
		Some(&server.base_url),
		false,
	)
	.output()
	.unwrap_or_else(|error| panic!("run trusted placeholder publish: {error}"));

	assert!(
		!output.status.success(),
		"a broken OIDC endpoint must fail the dart trusted publish\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	assert!(
		String::from_utf8_lossy(&output.stdout).trim().is_empty(),
		"failing placeholder publishes must not emit a report on stdout"
	);
	// The mint request targeted the pub.dev audience with the runner token.
	assert_eq!(server.request_count(OIDC_ROUTE), 1);
	let mint_request = server.mint_head();
	assert!(
		mint_request.contains("audience=https%3A%2F%2Fpub.dev"),
		"the mint must target the pub.dev audience, got: {mint_request}"
	);
	assert!(
		mint_request
			.to_ascii_lowercase()
			.contains("bearer runner-request-secret"),
		"expected the runner bearer token in the mint request, got: {mint_request}"
	);
	// No dart command may run when minting fails; the stub log is only created
	// by the stub itself, which test 3 installs.
	assert!(!dart_stub_log_path(&workspace).exists());
	assert_snapshot!(
		String::from_utf8_lossy(&output.stderr).replace(&server.base_url, "[mock-endpoints]")
	);
}

#[cfg(unix)]
#[test]
fn dart_trusted_publish_mints_a_fresh_oidc_token_before_publishing() {
	let server = spawn_endpoint_server(Some(http_ok_token_response(FRESH_ACTIONS_JWT)));
	let workspace = fixture_workspace();
	let log_path = dart_stub_log_path(&workspace);
	let mut command = placeholder_publish_command(
		workspace.path(),
		&server.base_url,
		Some(&server.base_url),
		false,
	);
	install_dart_stub(&workspace, &log_path, &mut command);
	let output = command
		.output()
		.unwrap_or_else(|error| panic!("run trusted placeholder publish: {error}"));

	assert!(
		output.status.success(),
		"the placeholder publish should mint a fresh token and publish\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	let outcomes = package_publish_outcomes(
		&String::from_utf8_lossy(&output.stdout),
		"placeholder publish",
	);
	assert_eq!(outcomes[0]["status"], "published");

	// The stub records exactly which commands monochange spawned and with which
	// PUB_TOKEN value, proving the fresh-token ordering end to end.
	let invocations = read_dart_invocations(&log_path);
	assert_eq!(
		invocations.len(),
		2,
		"expected dart pub token add and then dart pub publish, got: {invocations:#?}"
	);
	assert_eq!(invocations[0].pub_token, FRESH_ACTIONS_JWT);
	assert_eq!(
		invocations[0].args,
		DART_TOKEN_ADD_ARGS
			.iter()
			.map(|argument| argument.to_string())
			.collect::<Vec<_>>()
	);
	assert_eq!(invocations[1].pub_token, FRESH_ACTIONS_JWT);
	assert_eq!(
		invocations[1].args,
		DART_PUBLISH_ARGS
			.iter()
			.map(|argument| argument.to_string())
			.collect::<Vec<_>>()
	);

	// The OIDC endpoint received exactly one pub.dev-audience mint request with
	// the runner-provided bearer token.
	assert_eq!(server.request_count(OIDC_ROUTE), 1);
	let mint_request = server.mint_head();
	assert!(
		mint_request.contains("GET /token&audience=https%3A%2F%2Fpub.dev HTTP/1.1"),
		"unexpected mint request: {mint_request}"
	);
	assert!(
		mint_request
			.to_ascii_lowercase()
			.contains("bearer runner-request-secret"),
		"expected the runner bearer token in the mint request, got: {mint_request}"
	);
}

#[cfg(unix)]
#[test]
fn dart_trusted_publish_without_actions_oidc_context_publishes_without_minting() {
	let server = spawn_endpoint_server(None);
	let workspace = fixture_workspace();
	let log_path = dart_stub_log_path(&workspace);
	let mut command = placeholder_publish_command(workspace.path(), &server.base_url, None, false);
	install_dart_stub(&workspace, &log_path, &mut command);
	let output = command
		.output()
		.unwrap_or_else(|error| panic!("run placeholder publish: {error}"));

	assert!(
		output.status.success(),
		"the placeholder publish should succeed without the OIDC endpoint\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	let outcomes = package_publish_outcomes(
		&String::from_utf8_lossy(&output.stdout),
		"plain placeholder publish",
	);
	assert_eq!(outcomes[0]["status"], "published");

	let invocations = read_dart_invocations(&log_path);
	assert_eq!(
		invocations.len(),
		1,
		"only the publish command may run, got: {invocations:#?}"
	);
	assert_eq!(invocations[0].pub_token, "unset");
	assert_eq!(
		invocations[0].args,
		DART_PUBLISH_ARGS
			.iter()
			.map(|argument| argument.to_string())
			.collect::<Vec<_>>()
	);
	assert_eq!(server.request_count(OIDC_ROUTE), 0);
}
