//! Prerelease workflow coverage: release-note deltas, channel resets, and
//! floating-tag stability.
//!
//! These scenarios span several `prepare-release` runs against a git history, so
//! they assert on the observable artifacts (release records, note bodies, tags)
//! rather than a single command invocation.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_fixture(relative: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(relative), root);
	// `tag-release` verifies the ref is reachable from a configured release
	// branch, so the branch name must be pinned rather than inherited from the
	// machine's `init.defaultBranch`.
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

fn monochange(root: &Path, args: &[&str]) -> Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {args:?}: {error}"));
	assert!(
		output.status.success(),
		"monochange {args:?} failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!(
			"parse monochange {args:?} json: {error}\nstdout:\n{}",
			String::from_utf8_lossy(&output.stdout)
		)
	})
}

fn prepare_release(root: &Path) -> Value {
	monochange(root, &["prepare", "--format", "json"])
}

fn commit_all(root: &Path, message: &str) {
	git(root, &["add", "."]);
	git(root, &["commit", "-m", message]);
}

fn read_state(root: &Path) -> Value {
	let path = root.join(".monochange/prerelease-state.json");
	serde_json::from_slice(
		&std::fs::read(&path).unwrap_or_else(|error| panic!("read state: {error}")),
	)
	.unwrap_or_else(|error| panic!("parse state: {error}"))
}

/// Return the release-note body rendered for `owner` by a prepare run.
///
/// The manifest `changelogs` array is what providers select a hosted release
/// body from, so asserting on it covers the prerelease note contract. A `None`
/// result means the run produced no artifact for that owner, which happens when
/// a prerelease has no new changes to report.
fn notes_from(prepared: &Value, owner: &str, output: &str) -> Option<String> {
	prepared["changelogs"]
		.as_array()
		.unwrap_or_else(|| panic!("prepared changelogs should be an array"))
		.iter()
		.find(|changelog| {
			changelog["owner_id"] == json!(owner) && changelog["output"] == json!(output)
		})
		.map(|changelog| {
			normalize_commit_links(
				changelog["rendered"]
					.as_str()
					.unwrap_or_else(|| panic!("rendered notes should be a string")),
			)
		})
}

/// Replace commit SHAs with a stable placeholder so notes snapshots stay
/// deterministic across runs while preserving the surrounding structure.
///
/// Provenance lines embed the changeset's introducing commit, whose SHA depends
/// on the commit timestamp and therefore differs on every test run.
fn normalize_commit_links(contents: &str) -> String {
	contents
		.lines()
		.map(|line| {
			if line.contains("_Introduced in:_") {
				let indent = line.len() - line.trim_start().len();
				format!(
					"{}_Owner:_ monochange-tests · _Introduced in:_ [commit]",
					" ".repeat(indent)
				)
			} else {
				line.to_string()
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

#[test]
fn prerelease_notes_report_only_changesets_added_since_the_previous_prerelease() {
	let tempdir = setup_fixture("prerelease/release-notes-delta");
	let root = tempdir.path();

	// The first prerelease reports the changeset that is already pending.
	let first = prepare_release(root);
	assert_json_snapshot!(
		"prerelease_notes_delta_first_run",
		json!({
			"release_targets": first["release_targets"],
			"released_packages": first["released_packages"],
		})
	);
	let first_notes = notes_from(&first, "core", "release")
		.unwrap_or_else(|| panic!("the first prerelease reports the pending changeset"));
	assert_snapshot!("prerelease_notes_delta_first_notes", first_notes);
	assert_no_changelog_files_written(root);
	commit_all(root, "prepare alpha 0");
	// A repeat prerelease with no new changes reports nothing new, so the delta
	// produces no note artifact at all.
	let second = prepare_release(root);
	assert_json_snapshot!(
		"prerelease_notes_delta_second_run",
		json!({
			"release_targets": second["release_targets"],
			"notes": notes_from(&second, "core", "release"),
		})
	);
	commit_all(root, "prepare alpha 1");

	// A newly added changeset is the only thing the next prerelease reports: the
	// still-present `first.md` must not be repeated.
	std::fs::write(
		root.join(".changeset/second.md"),
		"---\ncore: patch\n---\n\n#### fix a follow-up bug\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	commit_all(root, "add a second changeset");

	let third = prepare_release(root);
	assert_json_snapshot!(
		"prerelease_notes_delta_third_run",
		json!({
			"release_targets": third["release_targets"],
			"covered_changesets": read_state(root)["release_note_changesets"],
		})
	);
	let third_notes = notes_from(&third, "core", "release")
		.unwrap_or_else(|| panic!("the third prerelease reports the new changeset"));
	assert_snapshot!("prerelease_notes_delta_third_notes", third_notes);
	assert!(
		!third_notes.contains("add the first feature"),
		"the already reported changeset must not repeat in later prerelease notes:\n{third_notes}"
	);
}

#[test]
fn prerelease_notes_ignore_edits_to_an_already_reported_changeset() {
	let tempdir = setup_fixture("prerelease/release-notes-delta");
	let root = tempdir.path();

	prepare_release(root);
	commit_all(root, "prepare alpha 0");

	// Editing the body of an already reported changeset is not a new change.
	std::fs::write(
		root.join(".changeset/first.md"),
		"---\ncore: minor\n---\n\n#### add the first feature, now with different wording\n",
	)
	.unwrap_or_else(|error| panic!("rewrite changeset: {error}"));
	commit_all(root, "edit the reported changeset");

	let edited = prepare_release(root);

	let notes = notes_from(&edited, "core", "release").unwrap_or_default();
	assert!(
		!notes.contains("different wording"),
		"an edited already-reported changeset must not reappear in prerelease notes:\n{notes}"
	);
}

#[test]
fn switching_the_prerelease_channel_restarts_the_counter_and_the_notes() {
	let tempdir = setup_fixture("prerelease/release-notes-delta");
	let root = tempdir.path();

	prepare_release(root);
	let alpha_state = read_state(root);
	assert_json_snapshot!(
		"prerelease_channel_switch_alpha_state",
		json!({
			"channel": alpha_state["channel"],
			"latest": alpha_state["packages"]["cargo:crates/core/Cargo.toml"]["latest_prerelease_version"],
		})
	);
	commit_all(root, "prepare alpha 0");

	let config_path = root.join("monochange.toml");
	let config = std::fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read config: {error}"))
		.replace("channel = \"alpha\"", "channel = \"beta\"");
	std::fs::write(&config_path, config).unwrap_or_else(|error| panic!("write config: {error}"));
	commit_all(root, "switch the prerelease channel");

	let beta = prepare_release(root);
	let beta_state = read_state(root);
	assert_json_snapshot!(
		"prerelease_channel_switch_beta_state",
		json!({
			"channel": beta_state["channel"],
			"release_targets": beta["release_targets"],
			"latest": beta_state["packages"]["cargo:crates/core/Cargo.toml"]["latest_prerelease_version"],
		})
	);
	// A channel switch is a new series, so the pending changes are presented again.
	let beta_notes = notes_from(&beta, "core", "release")
		.unwrap_or_else(|| panic!("a new channel presents the pending changes again"));
	assert!(
		beta_notes.contains("add the first feature"),
		"a channel switch should report the pending changes again:\n{beta_notes}"
	);
	assert_snapshot!("prerelease_channel_switch_beta_notes", beta_notes);
}

#[test]
fn prerelease_mode_skips_floating_tags_but_stable_releases_move_them() {
	let tempdir = setup_fixture("prerelease/floating-tags");
	let root = tempdir.path();

	// A stable release moves the aliases onto its release commit.
	prepare_release(root);
	commit_all(root, "prepare stable release");
	monochange(
		root,
		&[
			"step",
			"tag-release",
			"--from",
			"HEAD",
			"--push=false",
			"--format",
			"json",
		],
	);
	let stable_commit = git_output(root, &["rev-parse", "v1"]);
	assert_eq!(
		stable_commit,
		git_output(root, &["rev-parse", "HEAD"]),
		"a stable release should move the floating tag onto its commit"
	);

	// Switching to prerelease mode keeps the alias pinned to that stable commit.
	let config_path = root.join("monochange.toml");
	let config = std::fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read config: {error}"))
		.replace("enabled = false", "enabled = true");
	std::fs::write(&config_path, config).unwrap_or_else(|error| panic!("write config: {error}"));
	std::fs::write(
		root.join(".changeset/second.md"),
		"---\ncore: minor\n---\n\n#### another feature\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	commit_all(root, "prepare a prerelease");

	let prerelease = prepare_release(root);
	commit_all(root, "commit the prerelease");
	let report = monochange(
		root,
		&[
			"step",
			"tag-release",
			"--from",
			"HEAD",
			"--push=false",
			"--format",
			"json",
		],
	);

	// `tag_results[].target_commit` and `existing_commit` embed run-specific
	// SHAs; the assertions below cover commit identity, so the snapshot keeps the
	// stable fields that show which tags were created and which aliases moved.
	let tag_results = report["tag_results"]
		.as_array()
		.unwrap_or_else(|| panic!("tag_results should be an array"))
		.iter()
		.map(|result| {
			json!({
				"tag_name": result["tag_name"],
				"operation": result["operation"],
				"floating_results": result["floating_results"],
			})
		})
		.collect::<Vec<_>>();
	assert_json_snapshot!(
		"prerelease_floating_tags_tag_report",
		json!({
			"release_targets": prerelease["release_targets"],
			"tag_results": tag_results,
		})
	);
	assert_eq!(
		git_output(root, &["rev-parse", "v1"]),
		stable_commit,
		"a prerelease must not repoint a floating tag away from the stable commit"
	);
	assert!(
		!git_output(root, &["tag", "-l", "v1.1"]).is_empty(),
		"the minor alias must be created for a stable release"
	);
	assert_ne!(
		git_output(root, &["rev-parse", "HEAD"]),
		stable_commit,
		"the prerelease should be a distinct commit for this assertion to be meaningful"
	);
}

#[test]
fn prerelease_release_notes_can_be_disabled() {
	let tempdir = setup_fixture("prerelease/release-notes-delta");
	let root = tempdir.path();
	let config_path = root.join("monochange.toml");
	let config = std::fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read config: {error}"))
		.replace("release_notes = true", "release_notes = false");
	std::fs::write(&config_path, config).unwrap_or_else(|error| panic!("write config: {error}"));
	commit_all(root, "disable prerelease release notes");

	// `prepare --format json` emits the release manifest directly, so the note
	// artifacts live at the top level.
	let prepared = prepare_release(root);
	assert_json_snapshot!(
		"prerelease_notes_disabled",
		json!({
			"changelogs": prepared["changelogs"],
			"release_targets": prepared["release_targets"],
		})
	);
	assert!(
		prepared["changelogs"].as_array().is_some_and(Vec::is_empty),
		"`release_notes = false` must not render prerelease note artifacts: {prepared:#?}"
	);
}

fn assert_no_changelog_files_written(root: &Path) {
	assert!(
		!root.join("release-notes").exists(),
		"prerelease mode must not write changelog files when `changelog = false`"
	);
}

fn git_output(root: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8_lossy(&output.stdout).trim().to_string()
}
