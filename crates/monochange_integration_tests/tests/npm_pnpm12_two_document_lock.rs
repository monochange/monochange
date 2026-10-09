//! Integration coverage for pnpm 12 two-document lockfiles.
//!
//! pnpm 12 writes `pnpm-lock.yaml` as two YAML documents: an env document with
//! config and package-manager dependencies followed by the project lockfile.
//! A release must parse the whole stream, rewrite pinned versions in the
//! project document, and leave the env document and separators untouched.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use monochange_test_helpers::snapshot_settings;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_fixture(relative: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(relative), root);
	git(root, &["init", "--initial-branch", "main"]);
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
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.args(["prepare", "--format", "json"])
		.output()
		.unwrap_or_else(|error| panic!("run prepare: {error}"));
	assert!(
		output.status.success(),
		"prepare failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse prepare json: {error}"))
}

fn prepare_summary(root: &Path) -> Value {
	let manifest = prepare_release(root);
	let changed_files = manifest["changed_files"]
		.as_array()
		.unwrap_or_else(|| panic!("changed_files was not an array: {manifest:#?}"))
		.to_vec();
	let targets = manifest["release_targets"]
		.as_array()
		.unwrap_or_else(|| panic!("release_targets was not an array: {manifest:#?}"))
		.iter()
		.map(|target| {
			serde_json::json!({
				"id": target["id"],
				"version": target["version"],
			})
		})
		.collect::<Vec<_>>();
	serde_json::json!({
		"changed_files": changed_files,
		"targets": targets,
	})
}

#[test]
fn prepare_release_rewrites_pnpm12_two_document_lockfiles() {
	let tempdir = setup_fixture("npm/pnpm12-two-document-lock");
	let root = tempdir.path();

	snapshot_settings().bind(|| {
		assert_json_snapshot!(prepare_summary(root));
	});
	let lockfile_contents = std::fs::read_to_string(root.join("pnpm-lock.yaml"))
		.unwrap_or_else(|error| panic!("read pnpm-lock.yaml: {error}"));
	snapshot_settings().bind(|| {
		assert_snapshot!(lockfile_contents);
	});
}
