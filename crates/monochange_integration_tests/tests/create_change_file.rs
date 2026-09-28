//! Integration tests for `monochange create` and its `[cli.change]` wrapper.
//!
//! Dry-run must never mutate the workspace: the command's entire purpose is
//! writing release intent, so a dry run that writes would silently change
//! release planning for the next `prepare` invocation.

use std::fs;
use std::path::Path;
use std::process::Command;
use std::process::Output;

use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_workspace() -> TempDir {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/create-change-file/single-cargo/workspace");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

fn run_monochange(root: &Path, args: &[&str]) -> Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("TERM", "dumb")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"))
}

fn changeset_file_names(root: &Path) -> Vec<String> {
	let changeset_dir = root.join(".changeset");
	let Ok(entries) = fs::read_dir(&changeset_dir) else {
		return Vec::new();
	};
	let mut names = entries
		.into_iter()
		.filter_map(|entry| {
			let entry = entry.unwrap_or_else(|error| panic!("changeset entry: {error}"));
			if entry
				.file_type()
				.unwrap_or_else(|error| panic!("file type: {error}"))
				.is_file()
			{
				Some(entry.file_name().to_string_lossy().into_owned())
			} else {
				None
			}
		})
		.collect::<Vec<_>>();
	names.sort();
	names
}

/// The default changeset path embeds a unix timestamp; normalize it so the
/// snapshot asserts the path shape instead of the wall clock.
fn normalize_timestamps(stdout: &str) -> String {
	let pattern = regex::Regex::new(r"\.changeset/\d+-core\.md")
		.unwrap_or_else(|error| panic!("timestamp regex: {error}"));
	pattern
		.replace_all(stdout, ".changeset/[timestamp]-core.md")
		.into_owned()
}

#[test]
fn create_dry_run_writes_no_change_file() {
	let fixture = setup_workspace();
	let root = fixture.path();

	let output = run_monochange(
		root,
		&[
			"create",
			"--package",
			"core",
			"--bump",
			"minor",
			"--reason",
			"Add helper",
			"--dry-run",
		],
	);

	assert!(
		output.status.success(),
		"dry run must exit 0\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);

	let files = changeset_file_names(root);
	assert!(
		files.is_empty(),
		"dry run must not write changeset files, found {files:?}"
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	assert_snapshot!("create_dry_run_stdout", normalize_timestamps(&stdout));
}

#[test]
fn create_without_dry_run_writes_change_file() {
	let fixture = setup_workspace();
	let root = fixture.path();

	let output = run_monochange(
		root,
		&[
			"create",
			"--package",
			"core",
			"--bump",
			"minor",
			"--reason",
			"Add helper",
		],
	);

	assert!(
		output.status.success(),
		"create must exit 0\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);

	let files = changeset_file_names(root);
	assert_eq!(
		files.len(),
		1,
		"create must write exactly one changeset file"
	);
	let content = fs::read_to_string(root.join(".changeset").join(&files[0]))
		.unwrap_or_else(|error| panic!("read changeset: {error}"));
	assert_snapshot!("create_write_changeset_content", content);

	let stdout = String::from_utf8_lossy(&output.stdout);
	assert_snapshot!("create_write_stdout", normalize_timestamps(&stdout));
}

#[test]
fn run_change_dry_run_writes_no_change_file() {
	let fixture = setup_workspace();
	let root = fixture.path();

	let output = run_monochange(
		root,
		&[
			"run",
			"change",
			"--package",
			"core",
			"--bump",
			"minor",
			"--reason",
			"Add helper",
			"--dry-run",
		],
	);

	assert!(
		output.status.success(),
		"dry run must exit 0\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);

	let files = changeset_file_names(root);
	assert!(
		files.is_empty(),
		"dry run must not write changeset files, found {files:?}"
	);

	let stdout = String::from_utf8_lossy(&output.stdout);
	assert_snapshot!("run_change_dry_run_stdout", normalize_timestamps(&stdout));
}

#[test]
fn run_change_without_dry_run_writes_change_file() {
	let fixture = setup_workspace();
	let root = fixture.path();

	let output = run_monochange(
		root,
		&[
			"run",
			"change",
			"--package",
			"core",
			"--bump",
			"minor",
			"--reason",
			"Add helper",
		],
	);

	assert!(
		output.status.success(),
		"run change must exit 0\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);

	let files = changeset_file_names(root);
	assert_eq!(
		files.len(),
		1,
		"run change must write exactly one changeset file"
	);
	let content = fs::read_to_string(root.join(".changeset").join(&files[0]))
		.unwrap_or_else(|error| panic!("read changeset: {error}"));
	assert_snapshot!("run_change_write_changeset_content", content);
}
