//! Integration tests for plain-text JSON output and `--format json-min`.
//!
//! These tests pin two guarantees for commands that accept a `--format` choice:
//!
//! - `--format json` emits valid, styled-free plain text JSON (no ANSI colors,
//!   background colors, or other terminal styling) even when color output is
//!   forced with `CLICOLOR_FORCE`.
//! - `--format json-min` emits the same data minified, with no whitespace and
//!   no terminal styling.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;

fn fixture_path() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/json-plain-output/release-workspace")
}

fn setup_fixture() -> tempfile::TempDir {
	let tempdir = tempfile::TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "test"]);
	git(root, &["config", "user.email", "test@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);

	let output = Command::new("git")
		.current_dir(root)
		.env("GIT_AUTHOR_DATE", "2026-04-05T00:00:00Z")
		.env("GIT_COMMITTER_DATE", "2026-04-05T00:00:00Z")
		.args(["commit", "-m", "initial"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));
	assert!(
		output.status.success(),
		"git commit failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	tempdir
}

fn mc(root: &Path, args: &[&str]) -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.current_dir(root);
	command.env_remove("NO_COLOR");
	command.env_remove("RUST_LOG");
	command.env("CLICOLOR_FORCE", "1");
	command.env("MONOCHANGE_NO_PROGRESS", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command.args(args);
	command
}

fn run_stdout(root: &Path, args: &[&str]) -> String {
	let output = mc(root, args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"));
	assert!(
		output.status.success(),
		"command failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout =
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"));
	assert!(
		!stdout.contains('\u{1b}'),
		"JSON output must not contain ANSI escape codes:\n{stdout}"
	);
	stdout
}

fn normalize_root(root: &Path, output: &str) -> String {
	let canonical = std::fs::canonicalize(root).unwrap_or_else(|error| panic!("root: {error}"));
	let canonical_path = canonical.to_string_lossy();
	let root_path = root.to_string_lossy();
	output
		.replace(canonical_path.as_ref(), "[ROOT]")
		.replace(root_path.as_ref(), "[ROOT]")
}

#[test]
fn release_json_min_output_is_minified_plain_text_matching_pretty_payload() {
	let tempdir = setup_fixture();
	let root = tempdir.path();

	let pretty = run_stdout(root, &["run", "release", "--dry-run", "--format", "json"]);
	let minified = run_stdout(
		root,
		&["run", "release", "--dry-run", "--format", "json-min"],
	);

	let pretty_json: Value =
		serde_json::from_str(&pretty).unwrap_or_else(|error| panic!("pretty json: {error}"));
	let minified_json: Value =
		serde_json::from_str(&minified).unwrap_or_else(|error| panic!("minified json: {error}"));
	assert_eq!(
		pretty_json, minified_json,
		"json-min must carry exactly the same data as json"
	);

	assert_eq!(
		minified.trim_end().matches('\n').count(),
		0,
		"json-min must stay on one line:\n{minified}"
	);
	assert!(
		!minified.trim_end().contains(": "),
		"json-min must not contain pretty spacing:\n{minified}"
	);

	assert_json_snapshot!(json_release_summary(&minified_json));
}

fn json_release_summary(json: &Value) -> Value {
	serde_json::json!({
		"command": json["command"],
		"version": json["version"],
		"dry_run": json["dry_run"],
		"released_packages": json["released_packages"],
	})
}

#[test]
fn release_json_output_has_no_ansi_styling_when_color_is_forced() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(
		tempdir.path(),
		&["run", "release", "--dry-run", "--format", "json"],
	);

	let json: Value = serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("json: {error}"));
	assert_eq!(json["command"], serde_json::json!("release"));
	assert_eq!(json["version"], serde_json::json!("0.1.1"));
	assert_eq!(
		json["release_targets"][0]["tag_name"],
		serde_json::json!("core/v0.1.1")
	);
}

#[test]
fn release_json_min_output_has_no_ansi_styling_when_color_is_forced() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(
		tempdir.path(),
		&["run", "release", "--dry-run", "--format", "json-min"],
	);

	let json: Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|error| panic!("json-min must stay parseable with colors forced: {error}"));
	assert_eq!(json["dry_run"], serde_json::json!(true));
	assert_eq!(
		stdout.trim_end().matches('\n').count(),
		0,
		"json-min must be a single line:\n{stdout}"
	);
}

#[test]
fn step_config_json_min_output_is_minified_without_styling() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(tempdir.path(), &["step", "config", "--format", "json-min"]);

	let json: Value =
		serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("config json-min: {error}"));
	assert!(json["config"].is_object());
	assert_eq!(
		stdout.trim_end().matches('\n').count(),
		0,
		"config json-min must be a single line"
	);
	// Only the path header is snapshotted: the config body embeds multiline
	// template strings that would be unreadable inside a minified snapshot.
	let config_paths = serde_json::json!({
		"project_root": "[ROOT]",
		"config_path": "[ROOT]/monochange.toml",
	});
	let normalized_project_root = normalize_root(tempdir.path(), &json["project_root"].to_string());
	let normalized_config_path = normalize_root(tempdir.path(), &json["config_path"].to_string());
	assert_eq!(normalized_project_root, "\"[ROOT]\"");
	assert_eq!(normalized_config_path, "\"[ROOT]/monochange.toml\"");
	assert_json_snapshot!(config_paths);
}

#[test]
fn step_discover_json_min_output_is_minified_without_styling() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(
		tempdir.path(),
		&["step", "discover", "--format", "json-min"],
	);

	let json: Value =
		serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("discover json-min: {error}"));
	assert!(json["packages"].is_array());
	assert_eq!(
		stdout.trim_end().matches('\n').count(),
		0,
		"discovery json-min must be one line"
	);
	assert_json_snapshot!(json_discovery_summary(&json));
}

fn json_discovery_summary(json: &Value) -> Value {
	serde_json::json!({
		"package_ids": json["packages"]
			.as_array()
			.unwrap_or_else(|| panic!("packages was not an array: {json:#?}"))
			.iter()
			.filter_map(|package| package["id"].as_str())
			.collect::<Vec<_>>(),
		"warnings": json["warnings"],
	})
}

#[test]
fn versions_list_json_min_outputs_single_line_json() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(
		tempdir.path(),
		&["versions", "list", "--format", "json-min"],
	);
	assert_snapshot!(stdout);
}

#[test]
fn versions_list_json_min_via_inline_flag_outputs_single_line_json() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(tempdir.path(), &["versions", "list", "--format=json-min"]);
	let json: Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|error| panic!("inline json-min flag: {error}\n{stdout}"));
	assert_eq!(json["core"], serde_json::json!("0.1.0"));
	assert_eq!(stdout.trim_end().matches('\n').count(), 0);
}

#[test]
fn versions_sync_dry_run_json_min_output_is_minified() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(
		tempdir.path(),
		&["versions", "sync", "--dry-run", "--format", "json-min"],
	);

	let json: Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|error| panic!("versions sync json-min: {error}"));
	assert!(json["changes"].is_array());
	assert_eq!(
		stdout.trim_end().matches('\n').count(),
		0,
		"versions sync json-min must be one line"
	);
	assert_json_snapshot!(json);
}

#[test]
fn subagents_json_min_output_is_plain_text() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(
		tempdir.path(),
		&["subagents", "codex", "--dry-run", "--format", "json-min"],
	);

	let json: Value =
		serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("subagents json-min: {error}"));
	assert_eq!(json["dry_run"], serde_json::json!(true));
	assert_eq!(
		stdout.trim_end().matches('\n').count(),
		0,
		"subagents json-min must be a single line"
	);
	assert_json_snapshot!(json_subagents_summary(&json));
}

fn json_subagents_summary(json: &Value) -> Value {
	serde_json::json!({
		"targets": json["targets"],
		"dry_run": json["dry_run"],
		"file_count": json["files"].as_array().map(Vec::len).unwrap_or_default(),
	})
}

#[test]
fn check_json_output_has_no_ansi_styling_when_color_is_forced() {
	let tempdir = setup_fixture();
	let stdout = run_stdout(tempdir.path(), &["check", "--format", "json"]);

	let json: Value = serde_json::from_str(&stdout)
		.unwrap_or_else(|error| panic!("check json: {error}\n{stdout}"));
	assert!(json["results"].is_array());
}

#[test]
fn analyze_json_min_output_matches_pretty_payload_without_styling() {
	let tempdir = setup_fixture();
	let root = tempdir.path();

	let pretty = run_stdout(root, &["analyze", "--package", "core", "--format", "json"]);
	let minified = run_stdout(
		root,
		&["analyze", "--package", "core", "--format", "json-min"],
	);

	let pretty_json: Value =
		serde_json::from_str(&pretty).unwrap_or_else(|error| panic!("pretty json: {error}"));
	let minified_json: Value =
		serde_json::from_str(&minified).unwrap_or_else(|error| panic!("minified json: {error}"));
	assert_eq!(pretty_json, minified_json);
	assert_eq!(
		minified.trim_end().matches('\n').count(),
		0,
		"analyze json-min must be a single line"
	);
}

#[test]
fn unsupported_format_choice_fails_with_config_error() {
	let tempdir = setup_fixture();
	let output = mc(tempdir.path(), &["versions", "list", "--format", "yaml"])
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"));
	assert!(
		!output.status.success(),
		"unknown format must fail:\nstdout:\n{}",
		String::from_utf8_lossy(&output.stdout)
	);
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("invalid value 'yaml' for '--format <format>'"));
	assert!(stderr.contains("possible values: text, json, json-min"));
}
