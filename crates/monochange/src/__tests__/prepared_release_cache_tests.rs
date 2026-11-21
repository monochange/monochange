#![allow(clippy::disallowed_methods)]
use std::process::Command;

use monochange_config::load_workspace_configuration;
use monochange_test_helpers::fs::setup_scenario_workspace_from;
use tempfile::TempDir;

use super::*;

fn first_hash_for_path(path: &Path, hashes: Vec<String>) -> MonochangeResult<String> {
	hashes.into_iter().next().ok_or_else(|| {
		MonochangeError::Config(format!(
			"failed to hash {}: git returned no hash",
			path.display()
		))
	})
}

async fn hash_file_at_path(root: &Path, path: &Path) -> MonochangeResult<String> {
	let relative_path = root_relative(root, path);
	let relative = relative_path.to_string_lossy().into_owned();
	let hashes = hash_files_at_paths(root, &[relative]).await?;
	first_hash_for_path(path, hashes)
}

fn setup_prepared_release_repo() -> TempDir {
	let tempdir = setup_scenario_workspace_from(
		env!("CARGO_MANIFEST_DIR"),
		"prepared-release/source-github-follow-up",
	);
	let root = tempdir.path();
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "monochange tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(
		root,
		&["-c", "commit.gpgsign=false", "commit", "-m", "initial"],
	);

	tempdir
}

fn git(root: &Path, args: &[&str]) {
	let mut command = Command::new("git");
	command.current_dir(root).args(args);
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}
	let output = command
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn explicit_artifact_path(root: &Path) -> PathBuf {
	root.join(".monochange/local/unit-prepared-release.json")
}

#[test]
fn prepared_release_artifact_write_error_reports_io_and_serialization_context() {
	let path = Path::new("cache.json");
	let io_error = serde_json::Error::io(std::io::Error::other("disk full"));
	let message = prepared_release_artifact_write_error(path, &io_error).to_string();
	assert!(message.contains("failed to write prepared release artifact cache.json"));

	let serialization_error =
		serde_json::from_str::<serde_json::Value>("{invalid json").unwrap_err();
	let message = prepared_release_artifact_write_error(path, &serialization_error).to_string();
	assert!(message.contains("failed to serialize prepared release artifact"));
}

#[test]
fn render_unified_file_diff_includes_expected_headers() {
	let diff = render_unified_file_diff(
		Path::new("crates/core/Cargo.toml"),
		b"version = \"1.0.0\"\n",
		b"version = \"1.0.1\"\n",
	);
	assert!(diff.contains("--- a/crates/core/Cargo.toml"));
	assert!(diff.contains("+++ b/crates/core/Cargo.toml"));
	assert!(diff.contains("-version = \"1.0.0\""));
	assert!(diff.contains("+version = \"1.0.1\""));
}

#[tokio::test(flavor = "multi_thread")]
async fn load_prepared_release_execution_returns_cached_release_with_message_and_timings() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let prepared =
		crate::workspace_ops::prepare_release_execution_with_file_diffs(root, false, true, false)
			.await
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));
	save_prepared_release_execution(
		root,
		&configuration,
		&prepared.prepared_release,
		&prepared.file_diffs,
		Some(&artifact_path),
		"release",
	)
	.await
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let loaded =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, true)
			.await
			.unwrap_or_else(|error| panic!("load prepared release execution: {error}"))
			.unwrap_or_else(|| panic!("expected cached prepared release"));

	assert!(loaded.message.contains("reused prepared release artifact"));
	assert_eq!(loaded.originating_command.as_deref(), Some("release"));
	assert_eq!(loaded.execution.prepared_release, prepared.prepared_release);
	assert_eq!(loaded.execution.file_diffs, prepared.file_diffs);
	assert_eq!(loaded.execution.phase_timings.len(), 1);
	assert_eq!(
		loaded.execution.phase_timings[0].label,
		"load prepared release artifact"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn read_status_hash_and_ignore_helpers_report_non_git_cases() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let read_error = read_prepared_release_artifact(&root.join("missing.json"))
		.err()
		.unwrap_or_else(|| panic!("expected missing artifact error"));
	assert!(
		read_error
			.to_string()
			.contains("failed to read prepared release artifact")
	);

	let status_error = git_status_snapshot(root, None)
		.await
		.err()
		.unwrap_or_else(|| panic!("expected non-git status error"));
	assert!(
		status_error
			.to_string()
			.contains("failed to read git status")
	);

	let hash_error = hash_file_at_path(root, &root.join("missing.txt"))
		.await
		.err()
		.unwrap_or_else(|| panic!("expected hash-object failure"));
	assert!(hash_error.to_string().contains("failed to hash"));

	ensure_monochange_artifact_ignored(root, &root.join(".monochange/local/cache.json"))
		.await
		.unwrap_or_else(|error| panic!("non-git artifact ignore should succeed: {error}"));
}
