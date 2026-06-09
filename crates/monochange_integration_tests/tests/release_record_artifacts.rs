use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_release_fixture(relative: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(relative), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
	tempdir
}

fn prepare_release(root: &Path) -> Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.arg("step")
		.arg("prepare-release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run prepare-release: {error}"));
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse prepare-release json: {error}"))
}

fn release_record_paths(root: &Path) -> Vec<PathBuf> {
	let releases_dir = root.join(".monochange/releases");
	let Ok(entries) = std::fs::read_dir(&releases_dir) else {
		return Vec::new();
	};
	let mut paths = entries
		.map(|entry| {
			entry
				.unwrap_or_else(|error| panic!("read release entry: {error}"))
				.path()
				.join("release.json")
		})
		.filter(|path| path.exists())
		.collect::<Vec<_>>();
	paths.sort();
	paths
}

fn configure_release_command_for_empty_release_records(root: &Path) {
	let config_path = root.join("monochange.toml");
	let config = std::fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read config: {error}"));
	let config = config.replace(
		"default = \"text\"\n\n[[cli.release.steps]]",
		"default = \"text\"\n\n[[cli.release.inputs]]\nname = \"write_empty_release_record\"\ntype = \"boolean\"\ndefault = false\n\n[[cli.release.steps]]",
	);
	let config = config.replace(
		"type = \"PrepareRelease\"\ninputs = [\"format\"]",
		"type = \"PrepareRelease\"\nallow_empty_changesets = true\ninputs = { format = \"{{ inputs.format }}\", write_empty_release_record = \"{{ inputs.write_empty_release_record }}\" }",
	);
	std::fs::write(&config_path, config).unwrap_or_else(|error| panic!("write config: {error}"));
}

fn run_release(root: &Path, args: &[&str]) {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.arg("run")
		.arg("release")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run release: {error}"));
	assert!(
		output.status.success(),
		"release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn check_failure(root: &Path) -> String {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.arg("check")
		.output()
		.unwrap_or_else(|error| panic!("run check: {error}"));
	assert!(!output.status.success(), "check unexpectedly succeeded");
	String::from_utf8_lossy(&output.stderr).to_string()
}

fn redact_prerelease_state(mut state: Value) -> Value {
	state["created_at"] = Value::String("[timestamp]".to_string());
	state["updated_at"] = Value::String("[timestamp]".to_string());
	state
}

fn read_prerelease_state(root: &Path) -> Value {
	let state_path = root.join(".monochange/prerelease-state.json");
	serde_json::from_slice(
		&std::fs::read(&state_path).unwrap_or_else(|error| panic!("read state: {error}")),
	)
	.unwrap_or_else(|error| panic!("parse state: {error}"))
}

#[test]
fn prepare_release_supports_prerelease_without_changesets() {
	let cases = [
		("planned", "prerelease/no-changesets-planned"),
		("current-stable", "prerelease/no-changesets-current-stable"),
		("fixed", "prerelease/no-changesets-fixed"),
	];
	let mut outputs = serde_json::Map::new();

	for (case_name, fixture) in cases {
		let tempdir = setup_release_fixture(fixture);
		let root = tempdir.path();

		let prepared = prepare_release(root);
		let state = read_prerelease_state(root);
		outputs.insert(
			case_name.to_string(),
			serde_json::json!({
				"release_targets": prepared["release_targets"],
				"state": redact_prerelease_state(state),
			}),
		);
	}

	assert_json_snapshot!(Value::Object(outputs));
}

#[test]
fn prepare_release_increments_repeated_no_changeset_prereleases_from_json_state() {
	let tempdir = setup_release_fixture("prerelease/no-changesets-planned");
	let root = tempdir.path();

	let first = prepare_release(root);
	let first_state = read_prerelease_state(root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "prepare alpha 0"]);
	let second = prepare_release(root);
	let second_state = read_prerelease_state(root);

	assert_json_snapshot!(serde_json::json!({
		"first_release_targets": first["release_targets"],
		"firstState": redact_prerelease_state(first_state),
		"second_release_targets": second["release_targets"],
		"secondState": redact_prerelease_state(second_state),
	}));
}

#[test]
fn prepare_stable_release_removes_prerelease_state() {
	let tempdir = setup_release_fixture("prerelease/with-changesets-planned");
	let root = tempdir.path();

	let prerelease = prepare_release(root);
	assert!(root.join(".monochange/prerelease-state.json").exists());
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "prepare alpha 0"]);
	let config_path = root.join("monochange.toml");
	let config = std::fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read config: {error}"))
		.replace("enabled = true", "enabled = false");
	std::fs::write(&config_path, config).unwrap_or_else(|error| panic!("write config: {error}"));

	let stable = prepare_release(root);
	assert!(!root.join(".monochange/prerelease-state.json").exists());

	assert_json_snapshot!(serde_json::json!({
		"prerelease_targets": prerelease["release_targets"],
		"stable_targets": stable["release_targets"],
		"stateExistsAfterStable": root.join(".monochange/prerelease-state.json").exists(),
	}));
}

#[test]
fn check_rejects_stale_prerelease_state_when_mode_is_off() {
	let tempdir = setup_release_fixture("prerelease/stale-state-disabled");
	let root = tempdir.path();

	let stderr = check_failure(root).replace(root.to_string_lossy().as_ref(), "[workspace]");
	let relevant_lines = stderr
		.lines()
		.filter(|line| line.contains("prerelease state") || line.contains("prerelease-state.json"))
		.map(|line| line.replace("/private[workspace]", "[workspace]"))
		.collect::<Vec<_>>();

	assert_json_snapshot!(serde_json::json!({
		"relevantLines": relevant_lines,
	}));
}

#[test]
fn release_skips_record_artifacts_for_empty_plan_by_default() {
	let tempdir = setup_release_fixture("monochange/release-base");
	let root = tempdir.path();
	std::fs::remove_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("remove changesets: {error}"));
	configure_release_command_for_empty_release_records(root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "remove changesets"]);

	run_release(root, &[]);

	assert!(release_record_paths(root).is_empty());
	assert!(
		!root
			.join(".monochange/local/release-manifest.json")
			.exists()
	);
}

#[test]
fn release_writes_record_artifacts_for_empty_plan_when_requested() {
	let tempdir = setup_release_fixture("monochange/release-base");
	let root = tempdir.path();
	std::fs::remove_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("remove changesets: {error}"));
	configure_release_command_for_empty_release_records(root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "remove changesets"]);

	run_release(root, &["--write-empty-release-record"]);

	assert_eq!(release_record_paths(root).len(), 1);
	assert!(
		root.join(".monochange/local/release-manifest.json")
			.exists()
	);
}

#[test]
fn prepare_release_persists_one_record_and_reuses_it_on_later_runs() {
	let tempdir = setup_release_fixture("release-pr/ungrouped");
	let root = tempdir.path();

	let first = prepare_release(root);
	let first_paths = release_record_paths(root);
	assert_eq!(first_paths.len(), 1);
	let first_record = std::fs::read_to_string(&first_paths[0])
		.unwrap_or_else(|error| panic!("read {}: {error}", first_paths[0].display()));
	let index_path = root.join(".monochange/local/release-index.jsonl");
	let first_index = std::fs::read_to_string(&index_path)
		.unwrap_or_else(|error| panic!("read {}: {error}", index_path.display()));

	let second = prepare_release(root);
	let second_paths = release_record_paths(root);
	assert_eq!(second_paths, first_paths);
	let second_record = std::fs::read_to_string(&second_paths[0])
		.unwrap_or_else(|error| panic!("read {}: {error}", second_paths[0].display()));
	let second_index = std::fs::read_to_string(&index_path)
		.unwrap_or_else(|error| panic!("read {}: {error}", index_path.display()));

	assert_eq!(second_record, first_record);
	assert_eq!(second_index, first_index);
	assert!(first_index.contains("1b9c77930352f342"));
	assert_eq!(
		first["release_targets"], second["release_targets"],
		"repeat prepare-release should compute the same release target identities"
	);
	let index_entries = first_index
		.lines()
		.map(|line| {
			serde_json::from_str::<Value>(line)
				.unwrap_or_else(|error| panic!("parse index line `{line}`: {error}"))
		})
		.collect::<Vec<_>>();
	assert_json_snapshot!(serde_json::json!({
		"releaseRecordPath": first_paths[0]
			.strip_prefix(root)
			.unwrap_or_else(|error| panic!("strip temp root: {error}"))
			.to_string_lossy(),
		"index": index_entries,
		"release_targets": first["release_targets"],
	}));
}
