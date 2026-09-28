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

fn setup_cache_invalidation_repo() -> TempDir {
	let tempdir = setup_scenario_workspace_from(
		env!("CARGO_MANIFEST_DIR"),
		"prepared-release/cache-invalidation",
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

/// Copy a checked-in fixture payload into the workspace as a changeset.
fn install_changeset_payload(root: &Path, payload: &str, changeset_name: &str) -> PathBuf {
	let source = monochange_test_helpers::fs::fixture_path_from(
		env!("CARGO_MANIFEST_DIR"),
		"prepared-release/cache-invalidation/payloads",
	)
	.join(payload);
	let destination = root.join(".changeset").join(changeset_name);
	fs::create_dir_all(destination.parent().unwrap_or(root))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::copy(&source, &destination).unwrap_or_else(|error| {
		panic!(
			"copy fixture payload {} to {}: {error}",
			source.display(),
			destination.display()
		)
	});
	destination
}

/// Overwrite an installed changeset with another checked-in fixture payload.
fn replace_changeset_payload(root: &Path, payload: &str, changeset_name: &str) {
	install_changeset_payload(root, payload, changeset_name);
}

fn planned_versions(prepared_release: &PreparedRelease) -> Vec<(String, String)> {
	prepared_release
		.release_targets
		.iter()
		.map(|target| (target.id.clone(), target.version.clone()))
		.collect()
}

async fn prepare_with_configuration(
	root: &Path,
	configuration: &WorkspaceConfiguration,
) -> PreparedRelease {
	crate::workspace_ops::prepare_release_execution_with_configuration(
		root,
		configuration,
		true,
		false,
		false,
	)
	.await
	.unwrap_or_else(|error| panic!("prepare release execution: {error}"))
	.prepared_release
}

async fn save_artifact(root: &Path, dry_run: bool, explicit_path: &Path) -> WorkspaceConfiguration {
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let prepared = crate::workspace_ops::prepare_release_execution_with_file_diffs(
		root, dry_run, false, false,
	)
	.await
	.unwrap_or_else(|error| panic!("prepare release execution: {error}"));
	save_prepared_release_execution(
		root,
		&configuration,
		&prepared.prepared_release,
		&prepared.file_diffs,
		Some(explicit_path),
	)
	.await
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));
	configuration
}

#[test]
fn prepared_release_artifact_path_helpers_cover_default_and_explicit_paths() {
	let root = Path::new("/workspace");
	assert_eq!(
		default_prepared_release_cache_path(root),
		root.join(".monochange/local/prepared-release-cache.json")
	);
	assert_eq!(
		resolve_prepared_release_artifact_path(
			root,
			Some(Path::new(".monochange/local/custom.json"))
		),
		root.join(".monochange/local/custom.json")
	);
}

#[test]
fn read_prepared_release_artifact_reports_invalid_json() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let artifact_path = tempdir.path().join("artifact.json");
	fs::write(&artifact_path, "{invalid json")
		.unwrap_or_else(|error| panic!("write artifact: {error}"));
	let error = read_prepared_release_artifact(&artifact_path)
		.err()
		.unwrap_or_else(|| panic!("expected invalid json error"));
	assert!(
		error
			.to_string()
			.contains("failed to parse prepared release artifact")
	);
}

#[test]
fn configuration_snapshot_error_reports_serialization_context() {
	let error = serde_json::from_str::<serde_json::Value>("{invalid json").unwrap_err();
	let message = configuration_snapshot_error(&error).to_string();
	assert!(message.contains(CONFIGURATION_SNAPSHOT_ERROR));
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

#[tokio::test(flavor = "multi_thread")]
async fn hash_file_helpers_return_single_hash_and_reject_mismatched_output() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let path = root.join("hash-me.txt");
	fs::write(&path, "hash me\n").unwrap_or_else(|error| panic!("write hash fixture: {error}"));

	let hash = hash_file_at_path(root, &path)
		.await
		.unwrap_or_else(|error| panic!("hash file: {error}"));
	assert_eq!(hash.len(), 40);

	let error = parse_hash_object_output(b"abc123\n", 2)
		.err()
		.unwrap_or_else(|| panic!("expected mismatched hash count error"));
	assert!(
		error
			.to_string()
			.contains("failed to hash files: expected 2 hashes, got 1")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_status_snapshot_sorts_results_and_excludes_artifact_path() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	fs::create_dir_all(root.join(".monochange/local"))
		.unwrap_or_else(|error| panic!("mkdir .monochange: {error}"));
	fs::write(root.join(".monochange/local/cache.json"), "{}")
		.unwrap_or_else(|error| panic!("write cache: {error}"));
	fs::write(root.join("zzz.txt"), "z\n").unwrap_or_else(|error| panic!("write zzz: {error}"));
	fs::write(root.join("aaa.txt"), "a\n").unwrap_or_else(|error| panic!("write aaa: {error}"));

	let lines = git_status_snapshot(root, Some(&root.join(".monochange/local/cache.json")))
		.await
		.unwrap_or_else(|error| panic!("git status snapshot: {error}"));
	assert_eq!(
		lines,
		vec!["?? aaa.txt".to_string(), "?? zzz.txt".to_string()]
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn tracked_path_snapshots_deduplicate_paths_and_mark_deleted_entries() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let prepared =
		crate::workspace_ops::prepare_release_execution_with_file_diffs(root, true, false, false)
			.await
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));
	let changed_file = prepared
		.prepared_release
		.changed_files
		.first()
		.cloned()
		.unwrap_or_else(|| panic!("expected changed file"));
	let deleted_changeset = PathBuf::from(".changeset/deleted.md");
	let duplicate_path = changed_file.clone();

	save_prepared_release_execution(
		root,
		&configuration,
		&prepared.prepared_release,
		&prepared.file_diffs,
		Some(&explicit_artifact_path(root)),
	)
	.await
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let snapshots = tracked_path_snapshots(
		root,
		&PreparedRelease {
			changed_files: vec![changed_file.clone(), duplicate_path],
			deleted_changesets: vec![deleted_changeset.clone()],
			..prepared.prepared_release.clone()
		},
	)
	.await
	.unwrap_or_else(|error| panic!("tracked path snapshots: {error}"));

	assert_eq!(
		snapshots
			.iter()
			.filter(|snapshot| snapshot.path == changed_file)
			.count(),
		1
	);
	assert!(snapshots.iter().any(|snapshot| {
		snapshot.path == deleted_changeset
			&& snapshot.state == PreparedReleaseTrackedPathState::Deleted
			&& snapshot.hash.is_none()
	}));
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
async fn load_prepared_release_execution_rejects_non_dry_run_follow_up_from_dry_run_artifact() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, true, &artifact_path).await;

	let error =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, false)
			.await
			.unwrap_err();
	assert!(error.to_string().contains("dry-run artifacts"));
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
	)
	.await
	.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

	let loaded =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, true)
			.await
			.unwrap_or_else(|error| panic!("load prepared release execution: {error}"))
			.unwrap_or_else(|| panic!("expected cached prepared release"));

	assert!(loaded.message.contains("reused prepared release artifact"));
	assert_eq!(loaded.execution.prepared_release, prepared.prepared_release);
	assert_eq!(loaded.execution.file_diffs, prepared.file_diffs);
	assert_eq!(loaded.execution.phase_timings.len(), 1);
	assert_eq!(
		loaded.execution.phase_timings[0].label,
		"load prepared release artifact"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn load_prepared_release_execution_rejects_configuration_drift() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let _original_configuration = save_artifact(root, true, &artifact_path).await;
	fs::write(
		root.join("monochange.toml"),
		fs::read_to_string(root.join("monochange.toml"))
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"))
			.replacen("repo = \"monochange\"", "repo = \"monochange-next\"", 1),
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
	let changed_configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("reload workspace configuration: {error}"));

	let error = load_prepared_release_execution(
		root,
		&changed_configuration,
		Some(&artifact_path),
		true,
		false,
	)
	.await
	.unwrap_err();
	assert!(
		error
			.to_string()
			.contains("workspace configuration changed")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn load_prepared_release_execution_rejects_head_and_status_drift() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, false, &artifact_path).await;

	fs::write(root.join("README.md"), "head drift\n")
		.unwrap_or_else(|error| panic!("write README: {error}"));
	git(root, &["add", "README.md"]);
	git(
		root,
		&["-c", "commit.gpgsign=false", "commit", "-m", "head drift"],
	);

	let head_error =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, false)
			.await
			.unwrap_err();
	assert!(head_error.to_string().contains("HEAD changed"));

	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, false, &artifact_path).await;
	fs::write(root.join("README.md"), "status drift\n")
		.unwrap_or_else(|error| panic!("write README: {error}"));

	let status_error =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, false)
			.await
			.unwrap_err();
	assert!(
		status_error
			.to_string()
			.contains("workspace status no longer matches")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn load_prepared_release_execution_rejects_tracked_path_drift() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, false, &artifact_path).await;
	let mut artifact = read_prepared_release_artifact(&artifact_path)
		.unwrap_or_else(|error| panic!("read prepared release artifact: {error}"));
	let tracked = artifact
		.tracked_paths
		.iter_mut()
		.find(|tracked| tracked.state == PreparedReleaseTrackedPathState::File)
		.unwrap_or_else(|| panic!("expected tracked file entry"));
	tracked.hash = Some("not-the-real-hash".to_string());
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&artifact)
			.unwrap_or_else(|error| panic!("serialize artifact: {error}")),
	)
	.unwrap_or_else(|error| panic!("rewrite artifact: {error}"));
	let error =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, false)
			.await
			.unwrap_err();
	assert!(error.to_string().contains("workspace content drifted"));
}

#[tokio::test(flavor = "multi_thread")]
async fn load_prepared_release_execution_rejects_schema_mismatch() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, true, &artifact_path).await;
	let mut artifact = read_prepared_release_artifact(&artifact_path)
		.unwrap_or_else(|error| panic!("read prepared release artifact: {error}"));
	artifact.schema_version += 1;
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&artifact)
			.unwrap_or_else(|error| panic!("serialize artifact: {error}")),
	)
	.unwrap_or_else(|error| panic!("rewrite artifact: {error}"));

	let error =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), true, false)
			.await
			.unwrap_err();
	assert!(error.to_string().contains("schema version"));
}

#[tokio::test(flavor = "multi_thread")]
async fn maybe_load_prepared_release_execution_ignores_stale_default_cache() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let default_path = default_prepared_release_cache_path(root);
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let prepared =
		crate::workspace_ops::prepare_release_execution_with_file_diffs(root, false, false, false)
			.await
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));
	save_prepared_release_execution(
		root,
		&configuration,
		&prepared.prepared_release,
		&prepared.file_diffs,
		None,
	)
	.await
	.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));
	assert!(default_path.is_file());
	fs::write(
		root.join("crates/core/CHANGELOG.md"),
		"# Changelog\n\nautomatic drift\n",
	)
	.unwrap_or_else(|error| panic!("write drifted changelog: {error}"));

	let loaded = maybe_load_prepared_release_execution(root, &configuration, None, false, false)
		.await
		.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));
	assert!(loaded.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn maybe_load_prepared_release_execution_returns_explicit_stale_errors() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, false, &artifact_path).await;
	fs::write(root.join("README.md"), "explicit stale\n")
		.unwrap_or_else(|error| panic!("write README: {error}"));

	let error = maybe_load_prepared_release_execution(
		root,
		&configuration,
		Some(&artifact_path),
		false,
		false,
	)
	.await
	.unwrap_err();
	assert!(
		error
			.to_string()
			.contains("workspace status no longer matches")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn load_prepared_release_execution_rejects_missing_diff_previews_when_requested() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = save_artifact(root, false, &artifact_path).await;

	let error =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), false, true)
			.await
			.unwrap_err();
	assert!(error.to_string().contains("diff previews"));
}

#[test]
fn status_line_path_handles_short_and_standard_status_lines() {
	assert_eq!(status_line_path("??"), None);
	assert_eq!(status_line_path(" M Cargo.toml"), Some("Cargo.toml"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_monochange_artifact_ignored_updates_git_exclude_once() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = root.join(".monochange/local/cache.json");

	ensure_monochange_artifact_ignored(root, &artifact_path)
		.await
		.unwrap_or_else(|error| panic!("ensure artifact ignored: {error}"));
	ensure_monochange_artifact_ignored(root, &artifact_path)
		.await
		.unwrap_or_else(|error| panic!("ensure artifact ignored twice: {error}"));

	let exclude_path = root.join(".git").join("info").join("exclude");
	let exclude = fs::read_to_string(&exclude_path)
		.unwrap_or_else(|error| panic!("read git exclude: {error}"));
	assert_eq!(
		exclude
			.lines()
			.filter(|line| *line == ".monochange/local/")
			.count(),
		1
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn save_prepared_release_execution_reports_parent_and_write_failures() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let prepared =
		crate::workspace_ops::prepare_release_execution_with_file_diffs(root, false, false, false)
			.await
			.unwrap_or_else(|error| panic!("prepare release execution: {error}"));

	let parent_file = root.join("artifact-parent");
	fs::write(&parent_file, "file\n").unwrap_or_else(|error| panic!("write parent file: {error}"));
	let parent_error = save_prepared_release_execution(
		root,
		&configuration,
		&prepared.prepared_release,
		&prepared.file_diffs,
		Some(&parent_file.join("artifact.json")),
	)
	.await
	.unwrap_err();
	assert!(
		parent_error
			.to_string()
			.contains("failed to create prepared release artifact directory")
	);

	let artifact_dir = root.join(".monochange/local/write-error");
	fs::create_dir_all(&artifact_dir)
		.unwrap_or_else(|error| panic!("create artifact dir: {error}"));
	let write_error = save_prepared_release_execution(
		root,
		&configuration,
		&prepared.prepared_release,
		&prepared.file_diffs,
		Some(&artifact_dir),
	)
	.await
	.unwrap_err();
	assert!(
		write_error
			.to_string()
			.contains("failed to write prepared release artifact")
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

#[test]
fn first_hash_for_path_reports_empty_git_output() {
	let error = first_hash_for_path(Path::new("tracked.txt"), Vec::new())
		.err()
		.unwrap_or_else(|| panic!("expected empty hash output error"));

	assert!(error.to_string().contains("git returned no hash"));
}

#[tokio::test(flavor = "multi_thread")]
async fn git_status_snapshot_without_excluded_path_keeps_all_lines() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	fs::write(root.join("scratch.txt"), "scratch\n")
		.unwrap_or_else(|error| panic!("write scratch file: {error}"));

	let lines = git_status_snapshot(root, None)
		.await
		.unwrap_or_else(|error| panic!("git status snapshot without exclusions: {error}"));
	assert!(lines.iter().any(|line| line.ends_with("scratch.txt")));
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_monochange_artifact_ignored_skips_paths_outside_monochange_dir() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let artifact_path = root.join("prepared-release.json");
	let exclude_path = root.join(".git").join("info").join("exclude");

	ensure_monochange_artifact_ignored(root, &artifact_path)
		.await
		.unwrap_or_else(|error| panic!("ensure external artifact ignored: {error}"));
	let exclude = fs::read_to_string(&exclude_path).unwrap_or_default();
	assert!(!exclude.contains(".monochange/local/"));
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_monochange_artifact_ignored_appends_after_existing_content_without_newline() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let exclude_path = root.join(".git").join("info").join("exclude");
	fs::write(&exclude_path, "*.log")
		.unwrap_or_else(|error| panic!("seed git exclude file: {error}"));

	ensure_monochange_artifact_ignored(root, &root.join(".monochange/local/cache.json"))
		.await
		.unwrap_or_else(|error| panic!("append monochange ignore rule: {error}"));

	let exclude = fs::read_to_string(&exclude_path)
		.unwrap_or_else(|error| panic!("read git exclude file: {error}"));
	assert_eq!(exclude, "*.log\n.monochange/local/\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn ensure_monochange_artifact_ignored_reports_git_resolution_and_write_failures() {
	let removed_root = {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		tempdir.path().to_path_buf()
	};
	let resolve_error = ensure_monochange_artifact_ignored(
		&removed_root,
		&removed_root.join(".monochange/local/cache.json"),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected git exclude resolution error"));
	assert!(
		resolve_error
			.to_string()
			.contains("failed to resolve git exclude path")
	);

	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let exclude_path = root.join(".git").join("info").join("exclude");
	fs::remove_file(&exclude_path)
		.unwrap_or_else(|error| panic!("remove git exclude file: {error}"));
	fs::create_dir_all(&exclude_path)
		.unwrap_or_else(|error| panic!("create blocking exclude dir: {error}"));

	let write_error =
		ensure_monochange_artifact_ignored(root, &root.join(".monochange/local/cache.json"))
			.await
			.err()
			.unwrap_or_else(|| panic!("expected git exclude write error"));
	assert!(
		write_error
			.to_string()
			.contains("failed to update git exclude file")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn helper_error_paths_cover_hashing_and_git_exclude_directory_creation() {
	let removed_root = {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path().to_path_buf();
		fs::write(root.join("tracked.txt"), "tracked\n")
			.unwrap_or_else(|error| panic!("write tracked file: {error}"));
		root
	};
	let hash_error = hash_file_at_path(&removed_root, &removed_root.join("tracked.txt"))
		.await
		.err()
		.unwrap_or_else(|| panic!("expected hash spawn error"));
	assert!(hash_error.to_string().contains("failed to hash"));

	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let info_dir = root.join(".git/info");
	let backup_dir = root.join(".git/info-backup");
	fs::rename(&info_dir, &backup_dir)
		.unwrap_or_else(|error| panic!("move git info dir aside: {error}"));
	fs::write(&info_dir, "blocking file\n")
		.unwrap_or_else(|error| panic!("write blocking git info file: {error}"));

	let create_dir_error =
		ensure_monochange_artifact_ignored(root, &root.join(".monochange/local/cache.json"))
			.await
			.err()
			.unwrap_or_else(|| panic!("expected git exclude directory creation error"));
	assert!(
		create_dir_error
			.to_string()
			.contains("failed to create git exclude directory")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn tracked_path_snapshots_hashes_existing_changed_files() {
	let tempdir = setup_prepared_release_repo();
	let root = tempdir.path();
	let prepared_release = PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: root.to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: Vec::new(),
		package_publications: Vec::new(),
		version: None,
		group_version: None,
		release_targets: Vec::new(),
		changed_files: vec![PathBuf::from("monochange.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: false,
		versioning: crate::versioning_state::ResolvedReleaseValues::default(),
	};

	let tracked_paths = tracked_path_snapshots(root, &prepared_release)
		.await
		.unwrap_or_else(|error| panic!("tracked path snapshots: {error}"));

	assert_eq!(tracked_paths.len(), 1);
	assert_eq!(
		tracked_paths[0].state,
		PreparedReleaseTrackedPathState::File
	);
	assert!(tracked_paths[0].hash.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_invalidates_when_untracked_changeset_severity_changes_in_place() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let original = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &original, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	replace_changeset_payload(root, "alpha-major.md", "feature.md");
	let expected = prepare_with_configuration(root, &configuration).await;
	assert_ne!(
		planned_versions(&original),
		planned_versions(&expected),
		"fixture must plan a different version for the rewritten severity"
	);

	let loaded = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));

	assert!(
		loaded.is_none(),
		"a rewritten untracked changeset must not reuse the cached plan; loaded {loaded:?}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_invalidates_when_changeset_body_changes_without_severity_change() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let original = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &original, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	replace_changeset_payload(root, "alpha-minor-rewritten.md", "feature.md");
	let expected = prepare_with_configuration(root, &configuration).await;
	assert_eq!(
		planned_versions(&original),
		planned_versions(&expected),
		"fixture must keep the same severity so only the body changes"
	);
	assert_ne!(
		original.changesets, expected.changesets,
		"fixture must change the rendered changeset text"
	);

	let loaded = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));

	assert!(
		loaded.is_none(),
		"a rewritten changeset body must not reuse the cached plan"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_invalidates_when_changeset_is_added() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "alpha-minor.md");

	let original = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &original, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	install_changeset_payload(root, "beta-patch.md", "beta-patch.md");
	let expected_with_beta = prepare_with_configuration(root, &configuration).await;
	assert_ne!(
		planned_versions(&original),
		planned_versions(&expected_with_beta),
		"fixture must plan a different target set once the second changeset exists"
	);

	let loaded = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));
	assert!(
		loaded.is_none(),
		"adding a changeset must not reuse the cached plan"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_invalidates_when_changeset_is_removed() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "alpha-minor.md");
	let removed = install_changeset_payload(root, "beta-patch.md", "beta-patch.md");

	let original = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &original, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	fs::remove_file(&removed).unwrap_or_else(|error| panic!("remove changeset: {error}"));
	let expected_without_beta = prepare_with_configuration(root, &configuration).await;
	assert_ne!(
		planned_versions(&original),
		planned_versions(&expected_without_beta),
		"fixture must plan a different target set once the second changeset is gone"
	);

	let loaded = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));
	assert!(
		loaded.is_none(),
		"removing a changeset must not reuse the cached plan"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_hits_when_every_input_is_unchanged() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let prepared = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &prepared, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	let first = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("load prepared release execution: {error}"))
		.unwrap_or_else(|| panic!("an unchanged workspace must reuse the cached plan"));
	assert_eq!(first.execution.prepared_release, prepared);

	let second = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("load prepared release execution: {error}"))
		.unwrap_or_else(|| panic!("repeated identical runs must keep hitting the cache"));
	assert_eq!(second.execution.prepared_release, prepared);
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_invalidates_when_manifest_changes_without_a_status_change() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");
	let beta_manifest = root.join("crates/beta/Cargo.toml");

	// Dirty the manifest before saving so the workspace status line is already
	// present; a second edit keeps the same status letter and bytes are the only
	// signal left.
	fs::write(
		&beta_manifest,
		"# first edit\n[package]\nname = \"beta\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write first manifest edit: {error}"));
	let prepared = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &prepared, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	fs::write(
		&beta_manifest,
		"# second edit\n[package]\nname = \"beta\"\nversion = \"2.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write second manifest edit: {error}"));

	let loaded = maybe_load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.unwrap_or_else(|error| panic!("maybe load prepared release execution: {error}"));

	assert!(
		loaded.is_none(),
		"a changed package manifest must not reuse the cached plan"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_prepared_release_artifact_still_loads() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let prepared = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &prepared, &[], Some(&artifact_path))
		.await
		.unwrap_or_else(|error| panic!("save explicit prepared release artifact: {error}"));

	let loaded =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), true, false)
			.await
			.unwrap_or_else(|error| panic!("explicit artifact must load: {error}"))
			.unwrap_or_else(|| {
				panic!("an explicit artifact is a deliberate override and must load")
			});

	assert_eq!(loaded.execution.prepared_release, prepared);
	assert!(loaded.message.contains("reused prepared release artifact"));
}

#[tokio::test(flavor = "multi_thread")]
async fn implicit_cache_rejects_an_artifact_written_before_input_fingerprinting() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let prepared = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &prepared, &[], None)
		.await
		.unwrap_or_else(|error| panic!("save default prepared release artifact: {error}"));

	// Drop the fingerprint to imitate an artifact written by a binary that
	// predates input fingerprinting.
	let artifact_path = default_prepared_release_cache_path(root);
	let mut artifact = read_prepared_release_artifact(&artifact_path)
		.unwrap_or_else(|error| panic!("read prepared release artifact: {error}"));
	artifact.input_fingerprint = None;
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&artifact)
			.unwrap_or_else(|error| panic!("serialize artifact: {error}")),
	)
	.unwrap_or_else(|error| panic!("rewrite artifact: {error}"));

	let error = load_prepared_release_execution(root, &configuration, None, true, false)
		.await
		.expect_err("an artifact without a fingerprint cannot prove its plan is current");
	assert!(
		error
			.to_string()
			.contains("predates input fingerprinting and cannot prove its plan is current")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_artifact_without_a_fingerprint_still_loads() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let prepared = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &prepared, &[], Some(&artifact_path))
		.await
		.unwrap_or_else(|error| panic!("save explicit prepared release artifact: {error}"));

	let mut artifact = read_prepared_release_artifact(&artifact_path)
		.unwrap_or_else(|error| panic!("read prepared release artifact: {error}"));
	artifact.input_fingerprint = None;
	fs::write(
		&artifact_path,
		serde_json::to_string_pretty(&artifact)
			.unwrap_or_else(|error| panic!("serialize artifact: {error}")),
	)
	.unwrap_or_else(|error| panic!("rewrite artifact: {error}"));

	let loaded =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), true, false)
			.await
			.unwrap_or_else(|error| panic!("explicit artifact must load: {error}"))
			.unwrap_or_else(|| panic!("an explicit artifact is a deliberate override"));

	assert_eq!(loaded.execution.prepared_release, prepared);
}

#[tokio::test(flavor = "multi_thread")]
async fn explicit_artifact_with_a_drifted_fingerprint_still_loads() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let artifact_path = explicit_artifact_path(root);
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let prepared = prepare_with_configuration(root, &configuration).await;
	save_prepared_release_execution(root, &configuration, &prepared, &[], Some(&artifact_path))
		.await
		.unwrap_or_else(|error| panic!("save explicit prepared release artifact: {error}"));

	// Change the workspace inputs after the artifact was written.
	replace_changeset_payload(root, "alpha-major.md", "feature.md");

	let loaded =
		load_prepared_release_execution(root, &configuration, Some(&artifact_path), true, false)
			.await
			.unwrap_or_else(|error| panic!("explicit artifact must load: {error}"))
			.unwrap_or_else(|| panic!("an explicit artifact is a deliberate override"));

	assert_eq!(
		loaded.execution.prepared_release, prepared,
		"an explicit artifact must return the saved plan even after the inputs drift"
	);
}

#[test]
fn package_manifest_names_cover_every_package_type() {
	for (package_type, expected) in [
		(PackageType::Cargo, vec!["Cargo.toml"]),
		(PackageType::Npm, vec!["package.json"]),
		(PackageType::Deno, vec!["deno.json", "deno.jsonc"]),
		(PackageType::Dart, vec!["pubspec.yaml"]),
		(PackageType::Python, vec!["pyproject.toml"]),
		(PackageType::Go, vec!["go.mod"]),
		(
			PackageType::GitHubActions,
			vec!["action.yml", "action.yaml"],
		),
	] {
		assert_eq!(
			package_manifest_names(package_type),
			expected,
			"manifest names for `{}`",
			package_type.as_str()
		);
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn input_fingerprint_tracks_changeset_bytes_and_the_workspace_configuration() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	install_changeset_payload(root, "alpha-minor.md", "feature.md");

	let baseline = prepared_release_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("fingerprint inputs: {error}"));
	assert!(baseline.starts_with("fnv1a64:"));

	// Recomputing without touching anything must produce the same fingerprint so
	// the fast path keeps working.
	let repeated = prepared_release_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("fingerprint inputs twice: {error}"));
	assert_eq!(baseline, repeated);

	replace_changeset_payload(root, "alpha-major.md", "feature.md");
	let after_changeset_edit = prepared_release_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("fingerprint after changeset edit: {error}"));
	assert_ne!(
		baseline, after_changeset_edit,
		"changeset bytes must feed the fingerprint"
	);

	fs::remove_file(root.join(".changeset/feature.md"))
		.unwrap_or_else(|error| panic!("remove changeset: {error}"));
	let after_removal = prepared_release_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("fingerprint after removal: {error}"));
	assert_ne!(
		after_changeset_edit, after_removal,
		"the changeset set must feed the fingerprint"
	);

	fs::write(
		root.join("monochange.toml"),
		fs::read_to_string(root.join("monochange.toml"))
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"))
			.replace("changelog = ", "# rewritten\nchangelog = "),
	)
	.unwrap_or_else(|error| panic!("rewrite monochange.toml: {error}"));
	let after_configuration_edit = prepared_release_input_fingerprint(root, &configuration)
		.unwrap_or_else(|error| panic!("fingerprint after configuration edit: {error}"));
	assert_ne!(
		after_removal, after_configuration_edit,
		"the workspace configuration must feed the fingerprint"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn input_fingerprint_is_stable_across_equivalent_workspace_locations() {
	let first = setup_cache_invalidation_repo();
	let second = setup_cache_invalidation_repo();
	let first_configuration = load_workspace_configuration(first.path())
		.unwrap_or_else(|error| panic!("load first configuration: {error}"));
	let second_configuration = load_workspace_configuration(second.path())
		.unwrap_or_else(|error| panic!("load second configuration: {error}"));
	for root in [first.path(), second.path()] {
		install_changeset_payload(root, "alpha-minor.md", "feature.md");
	}

	let first_fingerprint = prepared_release_input_fingerprint(first.path(), &first_configuration)
		.unwrap_or_else(|error| panic!("fingerprint first workspace: {error}"));
	let second_fingerprint =
		prepared_release_input_fingerprint(second.path(), &second_configuration)
			.unwrap_or_else(|error| panic!("fingerprint second workspace: {error}"));

	assert_eq!(
		first_fingerprint, second_fingerprint,
		"the fingerprint must be workspace-relative so identical checkouts match"
	);
}

#[test]
fn fingerprint_fails_when_a_changeset_input_cannot_be_read() {
	let tempdir = setup_cache_invalidation_repo();
	let root = tempdir.path();
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));

	// A directory named like a changeset is discovered as an input but cannot be
	// read as a file, so the fingerprint reports the unreadable input instead of
	// silently hashing around it.
	let trap = root.join(".changeset").join("trap.md");
	fs::create_dir_all(&trap).unwrap_or_else(|error| panic!("create trap: {error}"));

	let error = prepared_release_input_fingerprint(root, &configuration)
		.err()
		.unwrap_or_else(|| panic!("expected the unreadable input to fail the fingerprint"));
	assert!(
		error
			.to_string()
			.contains("failed to read prepared release input"),
		"unexpected error: {error}"
	);
}

#[test]
fn package_manifest_inputs_stop_at_the_repository_root() {
	// An absolute package path that does not sit under the root must not walk
	// ancestors outside the workspace: the loop breaks before reading anything.
	let mut inputs = BTreeSet::new();
	let escaping = PackageDefinition {
		id: "escape".to_string(),
		path: PathBuf::from("/definitely/not/under/root"),
		package_type: PackageType::Cargo,
		changelog: None,
		excluded_changelog_types: Vec::new(),
		bump_propagation: None,
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: false,
		release: false,
		version_format: monochange_core::VersionFormat::Namespaced,
		version_source: monochange_core::VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,
		floating_tags: Vec::new(),
		publish: monochange_core::PublishSettings::default(),
		cli: None,
		values: std::collections::BTreeMap::new(),
		display_version: None,
	};
	insert_package_manifest_inputs(&mut inputs, Path::new("."), &escaping);
	assert!(inputs.is_empty(), "unexpected inputs: {inputs:?}");
}
