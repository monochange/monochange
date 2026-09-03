use std::path::Path;
use std::process::Command;
use std::process::Output;

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

fn run_notes(root: &Path, args: &[&str]) -> Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-09-02")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env_remove("RUST_LOG")
		.arg("notes")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("extract release notes: {error}"))
}

fn successful_notes(root: &Path, args: &[&str]) -> String {
	let output = run_notes(root, args);
	assert!(
		output.status.success(),
		"release notes failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("release notes stdout was not UTF-8: {error}"))
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

fn normalize_io_error(contents: &str) -> String {
	let operation = contents
		.rsplit_once(": ")
		.map_or(contents, |(operation, _error)| operation);
	let context = operation
		.rsplit_once(' ')
		.map_or(operation, |(context, _path)| context);
	format!("{context} [PATH]: [OS ERROR]")
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

#[test]
fn notes_prints_configured_markdown_text_and_json_outputs() {
	let workspace = setup_workspace();

	let markdown = successful_notes(workspace.path(), &["--output", "user_markdown"]);
	let text = successful_notes(workspace.path(), &["--output", "user_text"]);
	let json_output = successful_notes(workspace.path(), &["--output", "user_json"]);
	let json: serde_json::Value = serde_json::from_str(&json_output)
		.unwrap_or_else(|error| panic!("release notes JSON: {error}\n{json_output}"));

	assert_snapshot!(
		"notes_user_markdown_stdout",
		normalize_head_commit(workspace.path(), markdown)
	);
	assert_snapshot!(
		"notes_user_text_stdout",
		normalize_head_commit(workspace.path(), text)
	);
	assert_json_snapshot!("notes_user_json_stdout", json, {
		".sections[].entries[]" => "[multiline text]",
	});
	assert_snapshot!(
		"notes_user_json_entry",
		normalize_head_commit(
			workspace.path(),
			json["sections"][0]["entries"][0]
				.as_str()
				.unwrap_or_else(|| panic!("user JSON entry"))
				.to_string(),
		)
	);
}

#[test]
fn notes_target_selects_one_artifact_from_a_multi_target_output() {
	let workspace = setup_workspace();
	let app = successful_notes(
		workspace.path(),
		&["--output", "user_multi", "--target", "app"],
	);
	let group = successful_notes(
		workspace.path(),
		&["--output", "user_multi", "--target", "sdk"],
	);

	assert_snapshot!(
		"notes_multi_app_stdout",
		normalize_head_commit(workspace.path(), app)
	);
	assert_snapshot!(
		"notes_multi_group_stdout",
		normalize_head_commit(workspace.path(), group)
	);
}

#[test]
fn notes_supports_the_implicit_default_output() {
	let workspace = setup_workspace();
	let developer = successful_notes(
		workspace.path(),
		&["--output", "default", "--target", "core"],
	);

	assert_snapshot!(
		"notes_default_output_stdout",
		normalize_head_commit(workspace.path(), developer)
	);
}

#[test]
fn notes_file_writes_only_the_explicit_destination_without_preparing_the_release() {
	let workspace = setup_workspace();
	let destination = workspace.path().join("artifacts/user-notes.md");
	let output = run_notes(
		workspace.path(),
		&[
			"--output",
			"user_markdown",
			"--file",
			"artifacts/user-notes.md",
		],
	);

	assert!(output.status.success(), "notes --file failed: {output:#?}");
	assert!(output.stdout.is_empty());
	assert!(output.stderr.is_empty());
	assert!(workspace.path().join(".changeset/user.md").is_file());
	assert!(
		!workspace
			.path()
			.join("crates/app/release-notes/2.0.0.md")
			.exists()
	);
	assert!(
		std::fs::read_to_string(workspace.path().join("crates/app/Cargo.toml"))
			.unwrap_or_else(|error| panic!("app manifest: {error}"))
			.contains("version = \"1.0.0\"")
	);
	assert_snapshot!(
		"notes_explicit_file",
		normalize_head_commit(
			workspace.path(),
			std::fs::read_to_string(destination)
				.unwrap_or_else(|error| panic!("explicit notes file: {error}")),
		)
	);
}

#[test]
fn notes_file_supports_absolute_paths_and_reports_io_failures() {
	let workspace = setup_workspace();
	let destination = workspace.path().join("artifacts/absolute-notes.md");
	let destination_arg = destination
		.to_str()
		.unwrap_or_else(|| panic!("temporary destination is not UTF-8"));
	let absolute = run_notes(
		workspace.path(),
		&["--output", "user_markdown", "--file", destination_arg],
	);
	assert!(
		absolute.status.success(),
		"absolute notes failed: {absolute:#?}"
	);
	assert!(absolute.stdout.is_empty());
	assert!(destination.is_file());

	for (name, path) in [
		("create-parent", "monochange.toml/notes.md"),
		("write-directory", "."),
		("write-root", "/"),
	] {
		let output = run_notes(
			workspace.path(),
			&["--output", "user_markdown", "--file", path],
		);
		assert!(!output.status.success(), "{name} unexpectedly succeeded");
		assert!(output.stdout.is_empty());
		assert_snapshot!(
			format!("notes_error_{name}"),
			normalize_io_error(String::from_utf8_lossy(&output.stderr).trim())
		);
	}
}

#[test]
fn notes_file_dash_uses_stdout() {
	let workspace = setup_workspace();
	let output = successful_notes(
		workspace.path(),
		&["--output", "user_markdown", "--file", "-"],
	);

	assert_snapshot!(
		"notes_file_dash_stdout",
		normalize_head_commit(workspace.path(), output)
	);
}

#[test]
fn notes_quiet_mode_still_validates_without_printing() {
	let workspace = setup_workspace();
	let output = run_notes(workspace.path(), &["--quiet", "--output", "user_markdown"]);

	assert!(output.status.success(), "quiet notes failed: {output:#?}");
	assert!(output.stdout.is_empty());
	assert!(output.stderr.is_empty());
	assert!(workspace.path().join(".changeset/user.md").is_file());
}

#[test]
fn notes_reports_ambiguous_unknown_and_empty_selections() {
	let workspace = setup_workspace();
	let cases = [
		("ambiguous", vec!["--output", "user_multi"]),
		("unknown-output", vec!["--output", "missing"]),
		(
			"target-outside-output",
			vec!["--output", "user_markdown", "--target", "core"],
		),
		(
			"no-notes",
			vec!["--output", "developer_app", "--target", "app"],
		),
		("no-notes-without-target", vec!["--output", "developer_app"]),
	];

	for (name, args) in cases {
		let output = run_notes(workspace.path(), &args);
		assert!(!output.status.success(), "{name} unexpectedly succeeded");
		assert!(output.stdout.is_empty(), "{name} wrote stdout");
		assert_snapshot!(
			format!("notes_error_{name}"),
			String::from_utf8_lossy(&output.stderr).trim()
		);
	}
}
