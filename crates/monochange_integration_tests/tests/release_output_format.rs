use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/release-output-format/skipped-json-step")
}

fn setup_fixture() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
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

fn mc_release(root: &Path, args: &[&str]) -> String {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		.env_remove("RUST_LOG")
		.args(["run", "release", "--dry-run"])
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run mc release: {error}"));
	assert!(
		output.status.success(),
		"mc release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout was not utf8: {error}"))
}

#[test]
fn release_dry_run_keeps_markdown_output_when_skipped_step_inputs_request_json() {
	let tempdir = setup_fixture();
	let stdout = mc_release(tempdir.path(), &[]);

	assert!(!stdout.trim_start().starts_with('{'));
	assert_snapshot!(stdout);
}

#[test]
fn release_dry_run_still_honors_explicit_top_level_json_format() {
	let tempdir = setup_fixture();
	let stdout = mc_release(tempdir.path(), &["--format", "json"]);

	assert!(stdout.trim_start().starts_with('{'));
	let json: Value =
		serde_json::from_str(&stdout).unwrap_or_else(|error| panic!("release json: {error}"));
	assert_json_snapshot!(json_release_summary(&json));
}

fn json_release_summary(json: &Value) -> Value {
	let changelogs = json["changelogs"]
		.as_array()
		.unwrap_or_else(|| panic!("changelogs was not an array: {json:#?}"))
		.iter()
		.map(|changelog| {
			let sections = changelog["notes"]["sections"]
				.as_array()
				.unwrap_or_else(|| panic!("sections was not an array: {changelog:#?}"))
				.iter()
				.map(|section| {
					serde_json::json!({
						"title": section["title"],
						"entries": section["entries"].as_array().map(Vec::len).unwrap_or_default(),
					})
				})
				.collect::<Vec<_>>();
			serde_json::json!({
				"path": changelog["path"],
				"sections": sections,
				"rendered": "[see markdown output snapshot]",
			})
		})
		.collect::<Vec<_>>();

	serde_json::json!({
		"command": json["command"],
		"version": json["version"],
		"changelogs": changelogs,
	})
}
