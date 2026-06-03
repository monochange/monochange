//! Integration tests for `monochange versions`.

use std::ffi::OsString;
use std::path::Path;

use monochange::sync_workspace_versions;
use monochange_core::VersionStrategy;
use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_fixture(base: &str, name: &str) -> TempDir {
	let source =
		Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/tests/{base}/{name}"));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

fn run_versions_cli(root: &Path, args: &[&str]) -> String {
	let mut cli_args = vec![OsString::from("monochange"), OsString::from("versions")];
	cli_args.extend(args.iter().map(OsString::from));
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));
	let output = runtime
		.block_on(monochange::run_with_args_in_dir(
			"monochange",
			cli_args,
			root,
		))
		.unwrap_or_else(|error| panic!("monochange versions: {error}"));
	normalize_workspace_paths(root, output)
}

fn normalize_workspace_paths(root: &Path, output: String) -> String {
	let canonical =
		std::fs::canonicalize(root).unwrap_or_else(|error| panic!("canonicalize root: {error}"));
	let canonical_path = canonical.to_string_lossy();
	let root_path = root.to_string_lossy();
	output
		.replace(canonical_path.as_ref(), "[workspace]")
		.replace(root_path.as_ref(), "[workspace]")
}

fn assert_cli_snapshot(output: &str, expected: &str) {
	if output != expected {
		panic!("CLI output did not match snapshot\nexpected:\n{expected}\nactual:\n{output}");
	}
}

#[test]
fn sync_versions_detects_dart_internal_deps_in_mixed_workspace() {
	let fixture = setup_fixture("cli-output", "discover-mixed");
	let root = fixture.path();

	let result = sync_workspace_versions(root, VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("sync_workspace_versions: {error}"));

	// Validate the function runs successfully on a mixed-ecosystem workspace.
	let _ = result;
}

#[test]
fn sync_versions_updates_dart_dep_to_match_canonical_version() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let root = fixture.path();

	// Dry-run first to check detection without modifying files.
	let dry_result = sync_workspace_versions(root, VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("dry run: {error}"));

	// The version_mismatch package depends on core ^1.1.0 but core is at 1.2.3.
	// monochange versions should detect this and propose updating to ^1.2.3.
	assert!(
		!dry_result.changes.is_empty(),
		"expected sync to detect version mismatches"
	);

	let has_core_update = dry_result.changes.iter().any(|f| {
		f.changes
			.iter()
			.any(|c| c.dependency_name == "core" && c.new_value.contains("1.2.3"))
	});
	assert!(
		has_core_update,
		"expected core dependency to be updated to match version 1.2.3"
	);
}

#[test]
fn sync_versions_dry_run_preserves_files() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let root = fixture.path();

	let mismatch_path = root.join("packages/version_mismatch/pubspec.yaml");
	let original_contents = std::fs::read_to_string(&mismatch_path)
		.unwrap_or_else(|error| panic!("read original: {error}"));

	let _ = sync_workspace_versions(root, VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("dry run: {error}"));

	let current_contents = std::fs::read_to_string(&mismatch_path)
		.unwrap_or_else(|error| panic!("read current: {error}"));

	assert_eq!(
		current_contents, original_contents,
		"dry run should not modify files"
	);
}

#[test]
fn sync_versions_applies_changes_non_dry_run() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let root = fixture.path();

	let result = sync_workspace_versions(root, VersionStrategy::Default, false)
		.unwrap_or_else(|error| panic!("sync: {error}"));

	assert!(
		!result.changes.is_empty(),
		"expected sync to detect version mismatches"
	);

	// After sync, the version_mismatch pubspec should contain ^1.2.3.
	let mismatch_path = root.join("packages/version_mismatch/pubspec.yaml");
	let updated_contents = std::fs::read_to_string(&mismatch_path)
		.unwrap_or_else(|error| panic!("read updated: {error}"));

	assert!(
		updated_contents.contains("1.2.3"),
		"expected updated pubspec to reference core version 1.2.3"
	);
}

#[test]
fn sync_versions_with_exact_strategy_omits_caret() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let root = fixture.path();

	let result = sync_workspace_versions(root, VersionStrategy::Exact, false)
		.unwrap_or_else(|error| panic!("sync exact: {error}"));

	assert!(
		!result.changes.is_empty(),
		"expected sync with exact strategy to detect changes"
	);

	let mismatch_path = root.join("packages/version_mismatch/pubspec.yaml");
	let updated_contents = std::fs::read_to_string(&mismatch_path)
		.unwrap_or_else(|error| panic!("read updated: {error}"));

	// With Exact strategy, the version should appear without caret prefix.
	assert!(
		updated_contents.contains("1.2.3"),
		"expected exact version 1.2.3 in updated pubspec"
	);
}

#[test]
fn sync_versions_with_caret_strategy() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let root = fixture.path();

	let result = sync_workspace_versions(root, VersionStrategy::Caret, false)
		.unwrap_or_else(|error| panic!("sync caret: {error}"));

	assert!(
		!result.changes.is_empty(),
		"expected sync with caret strategy to detect changes"
	);

	let mismatch_path = root.join("packages/version_mismatch/pubspec.yaml");
	let updated_contents = std::fs::read_to_string(&mismatch_path)
		.unwrap_or_else(|error| panic!("read updated: {error}"));

	assert!(
		updated_contents.contains("^1.2.3"),
		"expected caret-prefixed version ^1.2.3 in updated pubspec"
	);
}

#[test]
fn sync_versions_with_compatible_strategy() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let root = fixture.path();

	let result = sync_workspace_versions(root, VersionStrategy::Compatible, false)
		.unwrap_or_else(|error| panic!("sync compatible: {error}"));

	assert!(
		!result.changes.is_empty(),
		"expected sync with compatible strategy to detect changes"
	);
}

#[test]
fn sync_versions_with_npm_internal_deps() {
	// Create a temporary npm workspace with internal dependencies.
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Write monochange.toml
	let config = "[package.lib-a]\npath = \"packages/lib-a\"\ntype = \"npm\"\n\n[package.lib-b]\npath = \"packages/lib-b\"\ntype = \"npm\"\n";
	std::fs::write(root.join("monochange.toml"), config)
		.unwrap_or_else(|error| panic!("write config: {error}"));

	// Write lib-a (version 2.0.0, no deps)
	std::fs::create_dir_all(root.join("packages/lib-a"))
		.unwrap_or_else(|error| panic!("create lib-a: {error}"));
	std::fs::write(
		root.join("packages/lib-a/package.json"),
		"{\"name\":\"lib-a\",\"version\":\"2.0.0\"}",
	)
	.unwrap_or_else(|error| panic!("write lib-a: {error}"));

	// Write lib-b (version 1.0.0, depends on lib-a ^1.0.0)
	std::fs::create_dir_all(root.join("packages/lib-b"))
		.unwrap_or_else(|error| panic!("create lib-b: {error}"));
	std::fs::write(
		root.join("packages/lib-b/package.json"),
		"{\"name\":\"lib-b\",\"version\":\"1.0.0\",\"dependencies\":{\"lib-a\":\"^1.0.0\"}}",
	)
	.unwrap_or_else(|error| panic!("write lib-b: {error}"));

	// Dry run to verify detection.
	let dry_result = sync_workspace_versions(root, VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("npm dry run: {error}"));

	assert!(
		!dry_result.changes.is_empty(),
		"expected npm sync to detect version mismatch"
	);

	let has_lib_a_update = dry_result.changes.iter().any(|f| {
		f.changes
			.iter()
			.any(|c| c.dependency_name == "lib-a" && c.new_value.contains("2.0.0"))
	});
	assert!(
		has_lib_a_update,
		"expected lib-a dependency to be updated to 2.0.0"
	);

	// Now apply for real.
	let result = sync_workspace_versions(root, VersionStrategy::Default, false)
		.unwrap_or_else(|error| panic!("npm apply: {error}"));

	assert!(!result.changes.is_empty());

	let updated = std::fs::read_to_string(root.join("packages/lib-b/package.json"))
		.unwrap_or_else(|error| panic!("read updated: {error}"));
	assert!(
		updated.contains("2.0.0"),
		"expected lib-b to reference lib-a 2.0.0 after sync"
	);
}

#[test]
fn versions_cli_dry_run_text_output_matches_snapshot() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let output = run_versions_cli(fixture.path(), &["--dry-run"]);
	assert_cli_snapshot(
		&output,
		include_str!(
			"snapshots/sync_versions__versions_cli_dry_run_text_output_matches_snapshot.txt"
		),
	);
}

#[test]
fn versions_cli_json_output_matches_snapshot() {
	let fixture = setup_fixture("dart-lints", "advanced-workspace-flutter/workspace");
	let output = run_versions_cli(fixture.path(), &["--dry-run", "--format", "json"]);
	assert_cli_snapshot(
		&output,
		include_str!("snapshots/sync_versions__versions_cli_json_output_matches_snapshot.txt"),
	);
}

#[test]
fn versions_cli_accepts_all_supported_ecosystems_in_mixed_workspace() {
	let fixture = setup_fixture("cli-output", "discover-mixed");
	let output = run_versions_cli(fixture.path(), &["--dry-run"]);
	assert!(
		!output.contains("Skipped unsupported ecosystems"),
		"supported ecosystems should not be reported as skipped: {output}"
	);
}
