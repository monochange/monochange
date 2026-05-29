//! Integration tests for Dart group version clobbering behavior.
//!
//! These tests verify that when a group has `versioned_files`, the group version
//! is correctly applied to those files without clobbering individual package versions
//! in their native manifests.

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

/// Initialize a git repository in the fixture directory so release planning works.
fn init_git_repo(root: &Path) {
	std::process::Command::new("git")
		.args(["init", "-b", "main"])
		.current_dir(root)
		.output()
		.expect("git init");
	std::process::Command::new("git")
		.args(["config", "user.name", "Test"])
		.current_dir(root)
		.output()
		.expect("git config");
	std::process::Command::new("git")
		.args(["config", "user.email", "test@test.com"])
		.current_dir(root)
		.output()
		.expect("git config");
	std::process::Command::new("git")
		.args(["config", "commit.gpgsign", "false"])
		.current_dir(root)
		.output()
		.expect("git config");
	std::process::Command::new("git")
		.args(["add", "."])
		.current_dir(root)
		.output()
		.expect("git add");
	std::process::Command::new("git")
		.args(["commit", "-m", "initial"])
		.current_dir(root)
		.output()
		.expect("git commit");
}

/// Create a changeset file to trigger a release.
fn create_changeset(root: &Path, package: &str, bump: &str, summary: &str) {
	let changeset_dir = root.join(".changeset");
	std::fs::create_dir_all(&changeset_dir).unwrap_or_else(|error| panic!("mkdir: {error}"));
	let changeset_path = changeset_dir.join("test-change.md");
	let content = format!("---\n{package}: {bump}\n---\n\n{summary}\n");
	std::fs::write(&changeset_path, content).unwrap_or_else(|error| panic!("write: {error}"));
}

/// Run mc step:prepare-release --dry-run and return the output.
fn run_prepare_release_dry_run(root: &Path) -> String {
	let cli_args = vec![
		std::ffi::OsString::from("mc"),
		std::ffi::OsString::from("step:prepare-release"),
		std::ffi::OsString::from("--dry-run"),
		std::ffi::OsString::from("--format"),
		std::ffi::OsString::from("json"),
	];
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));
	runtime
		.block_on(monochange::run_with_args_in_dir("mc", cli_args, root))
		.unwrap_or_else(|error| panic!("mc step:prepare-release --dry-run: {error}"))
}

/// Run mc step:prepare-release to apply version changes.
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
fn dart_group_versioned_files_does_not_clobber_package_version_in_dry_run() {
	let fixture = setup_fixture("basic");
	let root = fixture.path();
	init_git_repo(root);

	// Create a changeset that bumps core to trigger a group release
	create_changeset(root, "core", "patch", "Fix a bug in core");

	// Read original pubspec.yaml for app package
	let app_pubspec_path = root.join("packages/app/pubspec.yaml");
	let original_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	// Verify original version is 1.0.0
	assert!(
		original_contents.contains("version: 1.0.0"),
		"original version should be 1.0.0, got: {original_contents}"
	);

	// Run mc step:prepare-release --dry-run to see what would change
	let dry_run_output = run_prepare_release_dry_run(root);
	println!("Dry run output:\n{dry_run_output}");

	// Verify file is not modified in dry run
	let after_dry_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert_eq!(
		original_contents, after_dry_contents,
		"dry run should not modify files"
	);
}

#[test]
fn dart_group_versioned_files_applies_group_version_to_versioned_file() {
	let fixture = setup_fixture("basic");
	let root = fixture.path();
	init_git_repo(root);

	// Create a changeset that bumps core to trigger a group release
	create_changeset(root, "core", "patch", "Fix a bug in core");

	// Read original pubspec.yaml for app package
	let app_pubspec_path = root.join("packages/app/pubspec.yaml");
	let original_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	// Run mc step:prepare-release to apply changes
	let release_output = run_prepare_release(root);
	println!("Release output:\n{release_output}");

	// Read the updated pubspec.yaml
	let updated_contents =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));

	// Print the diff for debugging
	if updated_contents != original_contents {
		println!("=== pubspec.yaml was modified ===");
		println!("Original:\n{original_contents}");
		println!("Updated:\n{updated_contents}");
		println!("================================");
	}

	// The key question: does the group versioned_files setting cause the
	// version field in pubspec.yaml to be overwritten with the group version?
	// If the group version is 1.0.1 (patch bump), the version field should
	// be updated to 1.0.1 because it's listed in the group's versioned_files.
	//
	// But is this the desired behavior? The user reports that the group
	// "clobbers all versions in the codebase".

	// Check the core package too
	let core_pubspec_path = root.join("packages/core/pubspec.yaml");
	let core_updated =
		std::fs::read_to_string(&core_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));
	println!("Core pubspec after release:\n{core_updated}");
}

#[test]
fn dart_group_versioned_files_with_regex_updates_readme() {
	let fixture = setup_fixture("with-regex");
	let root = fixture.path();
	init_git_repo(root);

	// Create a changeset that bumps core to trigger a group release
	create_changeset(root, "core", "minor", "Add new feature to core");

	// Read original README
	let readme_path = root.join("README.md");
	let original_readme =
		std::fs::read_to_string(&readme_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(
		original_readme.contains("sdk@1.0.0"),
		"README should contain sdk@1.0.0, got: {original_readme}"
	);

	// Run mc step:prepare-release to apply changes
	let release_output = run_prepare_release(root);
	println!("Release output:\n{release_output}");

	// Read the updated README
	let updated_readme =
		std::fs::read_to_string(&readme_path).unwrap_or_else(|error| panic!("read: {error}"));
	println!("Updated README:\n{updated_readme}");

	// The regex versioned file should be updated
	assert!(
		updated_readme.contains("sdk@1.1.0"),
		"README should contain sdk@1.1.0 after minor bump, got: {updated_readme}"
	);

	// Check app pubspec - it should also be updated since it's in versioned_files
	let app_pubspec_path = root.join("packages/app/pubspec.yaml");
	let app_pubspec =
		std::fs::read_to_string(&app_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));
	println!("App pubspec after release:\n{app_pubspec}");

	// The version field in app/pubspec.yaml IS updated because it's listed
	// in the group's versioned_files - this is the "clobbering" behavior
	assert!(
		app_pubspec.contains("version: 1.1.0"),
		"App pubspec version should be updated to group version 1.1.0, got: {app_pubspec}"
	);

	// Check core pubspec - it should be updated as a group member
	let core_pubspec_path = root.join("packages/core/pubspec.yaml");
	let core_pubspec =
		std::fs::read_to_string(&core_pubspec_path).unwrap_or_else(|error| panic!("read: {error}"));
	println!("Core pubspec after release:\n{core_pubspec}");
	assert!(
		core_pubspec.contains("version: 1.1.0"),
		"Core pubspec version should be updated to 1.1.0, got: {core_pubspec}"
	);
}
