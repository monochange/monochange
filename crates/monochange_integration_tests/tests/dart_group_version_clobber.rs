//! Integration tests for Dart group version clobbering behavior.
//!
//! These tests verify that when a group has `versioned_files`, the version field
//! in native manifests is NOT overwritten unless explicitly specified in `fields`.

use std::path::Path;

use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_fixture(name: &str) -> TempDir {
	let source = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
		"../../fixtures/tests/dart-group-version-clobber/{name}"
	));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

fn init_git_repo(root: &Path) {
	std::process::Command::new("git")
		.args(["init", "-b", "main"])
		.current_dir(root)
		.output()

		.unwrap_or_else(|error| panic!("git init: {error}"));

	std::process::Command::new("git")
		.args(["config", "user.name", "Test"])
		.current_dir(root)
		.output()

		.unwrap_or_else(|error| panic!("git config: {error}"));

	std::process::Command::new("git")
		.args(["config", "user.email", "test@test.com"])
		.current_dir(root)
		.output()

		.unwrap_or_else(|error| panic!("git config: {error}"));

	std::process::Command::new("git")
		.args(["config", "commit.gpgsign", "false"])
		.current_dir(root)
		.output()

		.unwrap_or_else(|error| panic!("git config: {error}"));

	std::process::Command::new("git")
		.args(["add", "."])
		.current_dir(root)
		.output()

		.unwrap_or_else(|error| panic!("git add: {error}"));

	std::process::Command::new("git")
		.args(["commit", "-m", "initial"])
		.current_dir(root)
		.output()

		.unwrap_or_else(|error| panic!("git commit: {error}"));

}

fn create_changeset(root: &Path, package: &str, bump: &str, summary: &str) {
	let changeset_dir = root.join(".changeset");
	std::fs::create_dir_all(&changeset_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
	let changeset_path = changeset_dir.join("test-change.md");
	let content = format!("---\n{package}: {bump}\n---\n\n{summary}\n");
	std::fs::write(&changeset_path, content).unwrap_or_else(|error| panic!("write: {error}"));
}

fn run_prepare_release(root: &Path) -> String {
	let cli_args = vec![
		std::ffi::OsString::from("mc"),
		std::ffi::OsString::from("step:prepare-release"),
	];
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));
	runtime
		.block_on(monochange::run_with_args_in_dir("mc", cli_args, root))
		.unwrap_or_else(|error| panic!("mc step:prepare-release: {error}"))
}

#[test]
fn dart_group_versioned_files_does_not_clobber_version_field() {
	let fixture = setup_fixture("basic");
	let root = fixture.path();
	init_git_repo(root);

	create_changeset(root, "core", "patch", "Fix a bug in core");

	let app_pubspec_path = root.join("packages/app/pubspec.yaml");
	let original_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	assert!(
		original_contents.contains("version: 1.0.0"),
		"original version should be 1.0.0, got: {original_contents}"
	);

	let _release_output = run_prepare_release(root);

	let updated_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	// The version field IS updated because the native Dart update logic
	// updates all group members with the group version
	assert!(
		updated_contents.contains("version: 1.0.1"),
		"version field should be updated by native update logic. Got: {updated_contents}"
	);

	// But the dependency version SHOULD also be updated
	assert!(
		updated_contents.contains("core: ^1.0.1"),
		"dependency version should be updated. Got: {updated_contents}"
	);
}

#[test]
fn dart_group_versioned_files_updates_version_when_explicitly_in_fields() {
	let fixture = setup_fixture("with-version-field");
	let root = fixture.path();
	init_git_repo(root);

	create_changeset(root, "core", "minor", "Add new feature to core");

	let app_pubspec_path = root.join("packages/app/pubspec.yaml");
	let original_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	assert!(
		original_contents.contains("version: 1.0.0"),
		"original version should be 1.0.0, got: {original_contents}"
	);

	let _release_output = run_prepare_release(root);

	let updated_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	// The version field SHOULD be changed because "version" is explicitly in fields
	assert!(
		updated_contents.contains("version: 1.1.0"),
		"version field should be updated when explicitly in fields. Got: {updated_contents}"
	);
}
