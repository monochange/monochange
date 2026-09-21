//! Integration coverage for declared release values and version schemes.
//!
//! Each test copies a file fixture, runs the real CLI, and snapshots the
//! result. Dates are pinned with `MONOCHANGE_RELEASE_DATE` so calendar-derived
//! labels and ordinals are deterministic.

#![allow(clippy::disallowed_methods)]

use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_core::ReleaseManifest;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

const FIXTURE_ROOT: &str = "../../fixtures/tests/versioning";

/// Copy a fixture, initialise git, and commit so release planning has a base.
fn setup_workspace(name: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join(FIXTURE_ROOT)
		.join(name);
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

/// Run the CLI in `root` with a pinned release date.
fn run_cli(root: &Path, date: &str, args: &[&str]) -> Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", date)
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env_remove("RUST_LOG")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"))
}

fn stdout_of(output: &Output) -> String {
	String::from_utf8(output.stdout.clone())
		.unwrap_or_else(|error| panic!("stdout was not UTF-8: {error}"))
}

fn stderr_of(output: &Output) -> String {
	String::from_utf8(output.stderr.clone())
		.unwrap_or_else(|error| panic!("stderr was not UTF-8: {error}"))
}

/// Run prepare-release and require success.
fn prepare_release(root: &Path, date: &str) -> ReleaseManifest {
	let output = run_cli(root, date, &["step", "prepare-release", "--format", "json"]);
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("release manifest: {error}"))
}

/// Run prepare-release and require failure, returning the rendered diagnostic.
fn prepare_release_error(root: &Path, date: &str) -> String {
	let output = run_cli(root, date, &["step", "prepare-release", "--format", "json"]);
	assert!(
		!output.status.success(),
		"expected prepare-release to fail\nstdout:\n{}",
		stdout_of(&output)
	);
	stderr_of(&output)
}

fn read_file(root: &Path, relative: &str) -> String {
	let path = root.join(relative);
	fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Commit the working tree so a second prepare sees a clean base.
fn commit_all(root: &Path, message: &str) {
	git(root, &["add", "."]);
	git(root, &["commit", "-m", message]);
}

/// Restore a consumed changeset so a follow-up release can be planned.
fn write_changeset(root: &Path, name: &str, target: &str, bump: &str) {
	let directory = root.join(".changeset");
	fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("changeset dir: {error}"));
	let contents = format!("---\n\"{target}\": {bump}\n---\n\n# Follow-up\n\nDetails here.\n");
	fs::write(directory.join(name), contents)
		.unwrap_or_else(|error| panic!("write changeset: {error}"));
}

#[test]
fn train_scoped_counter_increments_within_a_version() {
	let workspace = setup_workspace("counter-train");
	let root = workspace.path();

	let manifest = prepare_release(root, "2026-09-19");

	assert_eq!(manifest.values.get("app.build"), Some(&"4".to_string()));
	assert_json_snapshot!("counter_train_manifest_values", &manifest.values);
	assert_snapshot!("counter_train_counter_file", read_file(root, "build.json"));
}

#[test]
fn train_scoped_counter_resets_when_the_version_changes() {
	let workspace = setup_workspace("counter-train");
	let root = workspace.path();

	prepare_release(root, "2026-09-19");
	commit_all(root, "release 1.1.0");
	write_changeset(root, "second.md", "app", "major");
	commit_all(root, "plan 2.0.0");

	// The version moves to 2.0.0, so an iOS-style train counter restarts.
	let manifest = prepare_release(root, "2026-09-20");

	assert_eq!(manifest.version.as_deref(), Some("2.0.0"));
	assert_eq!(manifest.values.get("app.build"), Some(&"1".to_string()));
	assert_snapshot!(
		"counter_train_reset_counter_file",
		read_file(root, "build.json")
	);
}

#[test]
fn owner_scoped_counter_never_resets() {
	let workspace = setup_workspace("counter-owner");
	let root = workspace.path();

	prepare_release(root, "2026-09-19");
	commit_all(root, "release 1.1.0");
	write_changeset(root, "second.md", "app", "major");
	commit_all(root, "plan 2.0.0");

	// `reset = "never"` matches Google Play: the value only ever increases.
	let manifest = prepare_release(root, "2026-09-20");

	assert_eq!(manifest.version.as_deref(), Some("2.0.0"));
	assert_eq!(manifest.values.get("app.build"), Some(&"5".to_string()));
}

#[test]
fn re_preparing_an_existing_record_does_not_advance_counters() {
	let workspace = setup_workspace("counter-train");
	let root = workspace.path();

	prepare_release(root, "2026-09-19");
	commit_all(root, "release 1.1.0");

	// The changeset was consumed, so restore an equivalent one. Re-running
	// prepare for the same version must reuse the frozen values rather than
	// stamping the counter a second time.
	write_changeset(root, "repeat.md", "app", "minor");
	commit_all(root, "replan");

	let first = prepare_release(root, "2026-09-19");
	let after_first = read_file(root, "build.json");
	let second = prepare_release(root, "2026-09-19");

	assert_eq!(first.values.get("app.build"), Some(&"1".to_string()));
	assert_eq!(
		second.values.get("app.build"),
		first.values.get("app.build")
	);
	assert_eq!(read_file(root, "build.json"), after_first);
	assert_snapshot!(
		"counter_train_reprepare_counter_file",
		read_file(root, "build.json")
	);
}

#[test]
fn value_templates_render_store_counters_into_manifests() {
	let workspace = setup_workspace("value-template");
	let root = workspace.path();

	let manifest = prepare_release(root, "2026-09-19");

	// Two counters on one app: the Android code never resets, the iOS build
	// resets with the version.
	assert_eq!(manifest.values.get("app.android"), Some(&"42".to_string()));
	assert_eq!(manifest.values.get("app.ios"), Some(&"8".to_string()));
	assert_json_snapshot!("value_template_manifest_values", &manifest.values);
	assert_snapshot!(
		"value_template_pubspec",
		read_file(root, "apps/app/pubspec.yaml")
	);
	assert_snapshot!(
		"value_template_store_json",
		read_file(root, "apps/app/store.json")
	);
	assert_snapshot!("value_template_counter_file", read_file(root, "build.json"));
}

#[test]
fn display_labels_render_calendar_schemes_per_package() {
	let workspace = setup_workspace("display-label");
	let root = workspace.path();

	let manifest = prepare_release(root, "2026-09-19");

	assert_eq!(manifest.labels.get("app"), Some(&"2026.09.1".to_string()));
	assert_eq!(manifest.labels.get("lib"), Some(&"26Q3.1".to_string()));
	assert_json_snapshot!("display_label_labels", &manifest.labels);
	let inputs = serde_json::json!({
		"date": manifest.label_inputs.date,
		"time": "[time]",
		"of_month": manifest.label_inputs.of_month,
		"of_quarter": manifest.label_inputs.of_quarter,
		"of_year": manifest.label_inputs.of_year,
	});
	assert_json_snapshot!("display_label_inputs", inputs);
}

#[test]
fn ordinals_chain_across_releases_in_the_same_month() {
	let workspace = setup_workspace("ordinal-chain");
	let root = workspace.path();

	let first = prepare_release(root, "2026-09-10");
	assert_eq!(first.labels.get("app"), Some(&"2026.09.1".to_string()));
	commit_all(root, "release 1");
	write_changeset(root, "second.md", "app", "patch");
	commit_all(root, "plan release 2");

	let second = prepare_release(root, "2026-09-11");
	assert_eq!(second.labels.get("app"), Some(&"2026.09.2".to_string()));

	write_changeset(root, "third.md", "app", "patch");
	commit_all(root, "plan release 3");

	// A new month restarts the monthly ordinal.
	let third = prepare_release(root, "2026-10-01");
	assert_eq!(third.labels.get("app"), Some(&"2026.10.1".to_string()));
	assert_json_snapshot!("ordinal_chain_labels", &third.labels);
}

#[test]
fn missing_counter_file_is_a_configuration_error() {
	let workspace = setup_workspace("missing-counter");
	let root = workspace.path();

	let error = prepare_release_error(root, "2026-09-19");

	assert!(error.contains("does not exist"), "{error}");
	assert!(
		error.contains("create it with its starting value"),
		"{error}"
	);
	assert_snapshot!("missing_counter_error", error);
}

#[test]
fn derived_sources_render_hash_env_and_timestamp_values() {
	let workspace = setup_workspace("derived-sources");
	let root = workspace.path();

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-09-19T10:11:12")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_TEST_RUN_NUMBER", "1842")
		.env_remove("RUST_LOG")
		.args(["step", "prepare-release", "--format", "json"])
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"));
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);
	let manifest: ReleaseManifest = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	// `digits` values are numeric, `base36` values are alphanumeric, and the
	// timestamp is fixed by the pinned release date.
	let artifact = manifest
		.values
		.get("app.artifact")
		.unwrap_or_else(|| panic!("artifact value missing"));
	assert_eq!(artifact.len(), 6);
	assert!(artifact.chars().all(|character| character.is_ascii_digit()));

	let tag = manifest
		.values
		.get("app.tag")
		.unwrap_or_else(|| panic!("tag value missing"));
	assert_eq!(tag.len(), 5);
	assert!(
		tag.chars()
			.all(|character| character.is_ascii_alphanumeric())
	);

	assert_eq!(manifest.values.get("app.run"), Some(&"1842".to_string()));
	assert_eq!(
		manifest.values.get("app.when"),
		Some(&"20260919101112".to_string())
	);
	assert_json_snapshot!("derived_sources_values", &manifest.values);
}

#[test]
fn missing_environment_variable_is_a_configuration_error() {
	let workspace = setup_workspace("derived-sources");
	let root = workspace.path();

	// The fixture reads MONOCHANGE_TEST_RUN_NUMBER, which is only set by the
	// success-path test.
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-09-19")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env_remove("RUST_LOG")
		.env_remove("MONOCHANGE_TEST_RUN_NUMBER")
		.args(["step", "prepare-release"])
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"));

	assert!(!output.status.success(), "expected the run to fail");
	let error = stderr_of(&output);
	assert!(error.contains("MONOCHANGE_TEST_RUN_NUMBER"), "{error}");
	assert_snapshot!("missing_env_error", error);
}

#[test]
fn dry_run_reports_values_without_writing_files() {
	let workspace = setup_workspace("value-template");
	let root = workspace.path();
	let before = read_file(root, "build.json");

	let output = run_cli(
		root,
		"2026-09-19",
		&["step", "prepare-release", "--dry-run", "--format", "json"],
	);
	assert!(
		output.status.success(),
		"dry-run prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);
	let manifest: ReleaseManifest = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	// A dry run resolves values for reporting but must not stamp counters.
	assert!(manifest.dry_run);
	assert_eq!(manifest.values.get("app.android"), Some(&"42".to_string()));
	assert_eq!(read_file(root, "build.json"), before);
	assert!(read_file(root, "apps/app/pubspec.yaml").contains("version: 1.0.0"));
}

#[test]
fn packages_without_values_leave_manifests_unchanged() {
	let workspace = setup_workspace("display-label");
	let root = workspace.path();

	// No declared values and no output paths, so the manifest carries no value
	// fields and no label inputs beyond the rendered labels.
	let manifest = prepare_release(root, "2026-09-19");

	assert!(manifest.values.is_empty());
	assert!(!manifest.labels.is_empty());
	let shape = serde_json::json!({
		"has_values": !manifest.values.is_empty(),
		"has_labels": !manifest.labels.is_empty(),
		"labels": manifest.labels,
	});
	assert_json_snapshot!("display_label_manifest_shape", shape);
}

#[test]
fn release_record_freezes_values_for_re_rendering() {
	let workspace = setup_workspace("counter-train");
	let root = workspace.path();

	prepare_release(root, "2026-09-19");
	commit_all(root, "release 1.1.0");

	// The committed record carries the frozen values, which is what makes a
	// later re-render stable.
	let record_path = find_release_record(root);
	let record: Value = serde_json::from_str(
		&fs::read_to_string(&record_path)
			.unwrap_or_else(|error| panic!("read record {}: {error}", record_path.display())),
	)
	.unwrap_or_else(|error| panic!("parse record: {error}"));
	assert_json_snapshot!(
		"release_record_frozen_values",
		serde_json::json!({
			"values": record.get("values").cloned().unwrap_or(Value::Null),
			"labels": record.get("labels").cloned().unwrap_or(Value::Null),
			"label_inputs": record.get("label_inputs").cloned().unwrap_or(Value::Null),
		})
	);
}

/// Locate the single committed release record in a workspace.
fn find_release_record(root: &Path) -> PathBuf {
	let releases = root.join(".monochange/releases");
	let mut records = Vec::new();
	for entry in fs::read_dir(&releases)
		.unwrap_or_else(|error| panic!("read {}: {error}", releases.display()))
	{
		let entry = entry.unwrap_or_else(|error| panic!("release entry: {error}"));
		let candidate = entry.path().join("release.json");
		if candidate.is_file() {
			records.push(candidate);
		}
	}
	assert_eq!(records.len(), 1, "expected exactly one release record");
	records.remove(0)
}
