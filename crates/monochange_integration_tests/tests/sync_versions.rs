//! Integration tests for `mc sync versions`.
//!
//! These tests require a properly-configured workspace fixture that
//! `discover_workspace` can find. They are ignored until the fixture
//! structure is validated.

use std::path::Path;

use monochange::sync_workspace_versions;
use monochange_core::VersionStrategy;
use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_sync_fixture(name: &str) -> TempDir {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join(format!("../../fixtures/tests/cli-output/{name}"));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

#[test]
#[ignore = "fixture discovery needs proper workspace structure"]
fn sync_versions_updates_dart_internal_deps() {
	let fixture = setup_sync_fixture("sync-versions-dart");
	let root = fixture.path();

	let result = sync_workspace_versions(root, VersionStrategy::Default, false)
		.unwrap_or_else(|error| panic!("sync_workspace_versions: {error}"));

	assert!(
		!result.changes.is_empty(),
		"expected changes to be detected"
	);
}
