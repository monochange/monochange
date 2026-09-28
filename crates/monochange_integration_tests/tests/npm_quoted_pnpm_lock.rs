//! Integration coverage for quoted pnpm lockfile keys.
//!
//! Real pnpm lockfiles quote scoped package names (`'@acme/api':`), and pnpm
//! regenerates and compares those files. A release must move the pinned version
//! even when the dependency key is quoted, and it must leave the key's quoting
//! alone. The fixture also carries `link:` and `workspace:` references, which
//! must survive untouched.

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

#[test]
fn prepare_release_rewrites_quoted_pnpm_lock_keys_and_keeps_their_quoting() {
	let tempdir = setup_fixture("npm/quoted-pnpm-lock");
	let root = tempdir.path();

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
	let summary = serde_json::json!({
		"changed_files": changed_files,
		"targets": targets,
	});
	snapshot_settings().bind(|| {
		assert_json_snapshot!(summary);
	});

	let lockfile = std::fs::read_to_string(root.join("pnpm-lock.yaml"))
		.unwrap_or_else(|error| panic!("read pnpm-lock.yaml: {error}"));
	snapshot_settings().bind(|| {
		assert_snapshot!(lockfile);
	});
}
