//! Integration coverage for the read-only next-version preview.
//!
//! `monochange next` answers "what will the next version be" from pending
//! changesets alone. It must never write release state: no `release.json`, no
//! prepared-release cache, and no changes to committed files.

use std::path::Path;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use monochange_test_helpers::git::git_output;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_next_version_preview_repo() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("next-version-preview"), root);
	git(root, &["init", "--initial-branch", "main"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn monochange_command() -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command.env("MONOCHANGE_NO_PROGRESS", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

fn run_monochange(root: &Path, args: &[&str]) -> String {
	let output = monochange_command()
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {args:?}: {error}"));

	assert!(
		output.status.success(),
		"monochange {args:?} failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("stdout utf8: {error}"))
}

/// Paths that release planning would write if the preview were not read-only.
fn release_state_paths() -> Vec<&'static str> {
	vec![
		"release.json",
		".monochange/local/prepared-release-cache.json",
		".monochange/local/release-manifest.json",
	]
}

fn assert_no_release_state(root: &Path) {
	for relative in release_state_paths() {
		assert!(
			!root.join(relative).exists(),
			"expected the preview to leave `{relative}` unwritten"
		);
	}

	let porcelain = git_output(root, &["status", "--porcelain", "--untracked-files=all"]);
	assert!(
		porcelain.trim().is_empty(),
		"expected a clean working tree after the preview, saw:\n{porcelain}"
	);
}

#[test]
fn next_reports_group_and_standalone_package_versions() {
	let tempdir = setup_next_version_preview_repo();
	let root = tempdir.path();

	let text = run_monochange(root, &["next"]);

	assert_eq!(
		text.trim_end(),
		"group versions:\n\
- sdk: 1.1.0\n\
package versions:\n\
- cargo:crates/sdk-a/Cargo.toml: 1.1.0\n\
- cargo:crates/sdk-b/Cargo.toml: 1.1.0\n\
- cargo:crates/tool/Cargo.toml: 1.0.1"
	);
	assert_no_release_state(root);
}

#[test]
fn next_reports_structured_versions_in_json() {
	let tempdir = setup_next_version_preview_repo();
	let root = tempdir.path();

	let json = run_monochange(root, &["next", "--format", "json"]);
	let parsed: Value = serde_json::from_str(&json)
		.unwrap_or_else(|error| panic!("parse next json: {error}\nraw:\n{json}"));

	assert_json_snapshot!("next_version_preview_json", parsed);
	assert_no_release_state(root);
}

#[test]
fn next_matches_display_versions_step_output() {
	let tempdir = setup_next_version_preview_repo();
	let root = tempdir.path();

	let alias_output = run_monochange(root, &["next", "--format", "json"]);
	let step_output = run_monochange(root, &["step", "display-versions", "--format", "json"]);

	assert_eq!(
		alias_output, step_output,
		"`monochange next` must stay identical to `monochange step display-versions`"
	);
	assert_no_release_state(root);
}

#[test]
fn next_reports_no_planned_versions_without_changesets() {
	let tempdir = setup_next_version_preview_repo();
	let root = tempdir.path();
	std::fs::remove_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("remove changesets: {error}"));
	git(root, &["add", "-A"]);
	git(root, &["commit", "-m", "drop changesets"]);

	let text = run_monochange(root, &["next"]);

	assert_eq!(
		text.trim_end(),
		"no package or group versions were planned",
		"an empty changeset set should report nothing planned rather than fail"
	);
	assert_no_release_state(root);
}
