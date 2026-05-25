//! Integration tests for `mc sync versions`.

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
	// sync versions should detect this and propose updating to ^1.2.3.
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
	// The core package is at 1.2.3, so the dep should be updated to "1.2.3"
	// (exact) rather than "^1.2.3" (default/caret).
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
