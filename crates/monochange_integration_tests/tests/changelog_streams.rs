use std::path::Path;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_core::ReleaseManifest;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::json;
use tempfile::TempDir;

fn setup_workspace() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/config/changelog-streams");
	copy_directory(&fixture, tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "test"]);
	git(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	git(tempdir.path(), &["config", "commit.gpgsign", "false"]);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "initial"]);
	tempdir
}

fn prepare_release(root: &Path) -> ReleaseManifest {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-09-02")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env_remove("RUST_LOG")
		.args(["step", "prepare-release", "--format", "json"])
		.output()
		.unwrap_or_else(|error| panic!("prepare release: {error}"));
	assert!(
		output.status.success(),
		"prepare release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("release manifest: {error}"))
}

fn normalize_head_commit(root: &Path, contents: String) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(["rev-parse", "--short", "HEAD"])
		.output()
		.unwrap_or_else(|error| panic!("read fixture HEAD: {error}"));
	assert!(output.status.success(), "git rev-parse failed");
	let commit = String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("fixture HEAD is not UTF-8: {error}"));
	contents.replace(&format!("`{}`", commit.trim()), "`[COMMIT]`")
}

#[test]
fn release_outputs_filter_changesets_by_stream() {
	let workspace = setup_workspace();
	let manifest = prepare_release(workspace.path());
	let changelog_identities = manifest
		.changelogs
		.iter()
		.map(|changelog| {
			json!({
				"owner": changelog.owner_id,
				"output": changelog.output,
				"stream": changelog.stream,
				"format": changelog.format,
				"path": changelog.path,
			})
		})
		.collect::<Vec<_>>();
	assert_json_snapshot!(changelog_identities);

	let user_notes = normalize_head_commit(
		workspace.path(),
		std::fs::read_to_string(workspace.path().join("crates/app/release-notes/2.0.0.json"))
			.unwrap_or_else(|error| panic!("user notes: {error}")),
	);
	let developer_notes = normalize_head_commit(
		workspace.path(),
		std::fs::read_to_string(workspace.path().join("crates/core/CHANGELOG.md"))
			.unwrap_or_else(|error| panic!("developer notes: {error}")),
	);
	let user_text = normalize_head_commit(
		workspace.path(),
		std::fs::read_to_string(workspace.path().join("crates/app/release-notes/2.0.0.txt"))
			.unwrap_or_else(|error| panic!("user text notes: {error}")),
	);
	let user_group = normalize_head_commit(
		workspace.path(),
		std::fs::read_to_string(workspace.path().join("release-notes/sdk/2.0.0.json"))
			.unwrap_or_else(|error| panic!("user group notes: {error}")),
	);
	let developer_append = normalize_head_commit(
		workspace.path(),
		std::fs::read_to_string(workspace.path().join("crates/core/INTERNAL.md"))
			.unwrap_or_else(|error| panic!("developer append notes: {error}")),
	);
	let user_notes = serde_json::from_str::<serde_json::Value>(&user_notes)
		.unwrap_or_else(|error| panic!("parse user notes: {error}"));
	let user_group = serde_json::from_str::<serde_json::Value>(&user_group)
		.unwrap_or_else(|error| panic!("parse user group notes: {error}"));
	assert_json_snapshot!("user_release_notes", user_notes, {
		".sections[].entries[]" => "[multiline text]",
	});
	assert_snapshot!(
		"user_release_note_entry",
		user_notes["sections"][0]["entries"][0]
			.as_str()
			.unwrap_or_else(|| panic!("user release-note entry"))
	);
	assert_snapshot!("user_text_release_notes", user_text);
	assert_json_snapshot!("user_group_release_notes", user_group, {
		".sections[].entries[]" => "[multiline text]",
	});
	assert_snapshot!(
		"user_group_release_note_entry",
		user_group["sections"][0]["entries"][0]
			.as_str()
			.unwrap_or_else(|| panic!("user group release-note entry"))
	);
	assert_snapshot!("developer_release_notes", developer_notes);
	assert_snapshot!("developer_append_release_notes", developer_append);
}
