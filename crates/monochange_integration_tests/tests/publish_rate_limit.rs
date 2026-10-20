//! A registry rate limit that stops a publish run reads as one clear failure.
//!
//! A stub `cargo` uploads the first crate and answers every later upload with
//! crates.io's real 429 response. The run must name the registry's reason and
//! retry time, say how to resume, and keep cargo's packaging log out of the
//! terminal.

#![allow(clippy::disallowed_methods)]

use std::path::Path;
use std::process::Command;

use httpmock::MockServer;
use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use regex::Regex;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_workspace() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/cli-output/publish-rate-limit"),
		tempdir.path(),
	);
	tempdir
}

#[cfg(unix)]
fn placeholder_publish(
	workspace: &Path,
	server: &MockServer,
	arguments: &[&str],
) -> std::process::Output {
	use std::os::unix::fs::PermissionsExt;

	let stub_dir = workspace.join("stub-bin");
	std::fs::set_permissions(
		stub_dir.join("cargo"),
		std::fs::Permissions::from_mode(0o755),
	)
	.unwrap_or_else(|error| panic!("stub permissions: {error}"));
	let path = std::env::var("PATH").unwrap_or_default();

	Command::new(get_cargo_bin("monochange"))
		.current_dir(workspace)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("PATH", format!("{}:{path}", stub_dir.display()))
		.env("RATE_LIMIT_STUB_STATE", workspace.join("stub-state"))
		.env("MONOCHANGE_CRATES_IO_INDEX_URL", server.base_url())
		.env(
			"MONOCHANGE_CRATES_IO_API_URL",
			format!("{}/api/v1", server.base_url()),
		)
		.args(arguments)
		.args(["step", "placeholder-publish"])
		.output()
		.unwrap_or_else(|error| panic!("run placeholder publish: {error}"))
}

fn not_found_registry() -> MockServer {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.any_request();
		then.status(404);
	});
	server
}

#[cfg(unix)]
#[test]
fn rate_limited_publish_names_the_retry_time_and_how_to_resume() {
	let server = not_found_registry();
	let workspace = fixture_workspace();

	let output = placeholder_publish(workspace.path(), &server, &[]);

	assert!(!output.status.success(), "a rate-limited run must fail");
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		!stderr.contains("Packaging rate_limit_layout"),
		"cargo's packaging log stays out of the terminal:\n{stderr}"
	);
	let durations =
		Regex::new(r"(?m)\d+(\.\d+)?(µs|ms|s)$").unwrap_or_else(|error| panic!("{error}"));
	assert_snapshot!(durations.replace_all(stderr.trim(), "[duration]"));
}

#[cfg(unix)]
#[test]
fn rate_limited_publish_reports_the_retry_time_as_json_progress() {
	let server = not_found_registry();
	let workspace = fixture_workspace();

	let output = placeholder_publish(workspace.path(), &server, &["--progress-format", "json"]);

	assert!(!output.status.success(), "a rate-limited run must fail");
	let events = String::from_utf8_lossy(&output.stderr)
		.lines()
		.filter_map(|line| serde_json::from_str::<Value>(line).ok())
		.collect::<Vec<_>>();
	let failed = events
		.iter()
		.find(|event| event["event"] == "publish_package_failed")
		.unwrap_or_else(|| panic!("expected a publish_package_failed event: {events:#?}"));
	assert_eq!(
		failed["message"],
		"rate limited by crates.io until Tue 20 Oct 2020 10:59:10 UTC. You have published too \
		 many new crates in a short period of time."
	);
	let diagnostic = events
		.iter()
		.find(|event| event["event"] == "diagnostic")
		.unwrap_or_else(|| panic!("expected a diagnostic event: {events:#?}"));
	assert_json_snapshot!(diagnostic, { ".detail" => "[multiline text]" });
	assert_snapshot!(
		"rate_limited_publish_diagnostic_detail",
		diagnostic["detail"].as_str().unwrap_or_default()
	);
}
