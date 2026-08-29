//! Integration tests for per-package and per-group `bump_propagation`.
//!
//! Declarations describe how a target's own changes propagate to its
//! dependents.
//!
//! The first fixture covers source-side policy:
//!
//! - `core` (source): `inherit` clamped to `minor`
//! - `engine` (depends on core): inherits the clamped severity
//! - `summary` (depends on engine): no declaration, falls back to
//!   `[defaults].parent_bump`
//! - `leaf` (source): `none`, so its dependent `standalone` never releases
//!
//! The second fixture covers the group layer: with no package declaration,
//! the group's own declaration applies, overriding the defaults layer. The
//! third covers the defaults layer: `[defaults].bump_propagation` applies to
//! packages with no declaration of their own (and no group), riding above
//! the `parent_bump` floor.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use tempfile::TempDir;

fn fixture_path(name: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/release-planning")
		.join(name)
		.join("workspace")
}

fn setup_workspace(name: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path(name), tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "test"]);
	git(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	git(tempdir.path(), &["config", "commit.gpgsign", "false"]);
	git(tempdir.path(), &["add", "."]);

	let output = Command::new("git")
		.current_dir(tempdir.path())
		.env("GIT_AUTHOR_DATE", "2026-04-05T00:00:00Z")
		.env("GIT_COMMITTER_DATE", "2026-04-05T00:00:00Z")
		.args(["commit", "-m", "initial"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));
	assert!(
		output.status.success(),
		"git commit failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	tempdir
}

fn mc_release(root: &Path, args: &[&str]) -> serde_json::Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.args(["run", "release", "--dry-run"])
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange release: {error}"));
	assert!(
		output.status.success(),
		"monochange release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout = String::from_utf8_lossy(&output.stdout).to_string();
	serde_json::from_str(&stdout)
		.unwrap_or_else(|error| panic!("release json was not valid JSON: {error}\n{stdout}"))
}

/// Project the planned decisions into a stable, path-free shape.
fn plan_decisions(json: &serde_json::Value) -> serde_json::Value {
	let decisions = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("plan.decisions was not an array: {json:#?}"));
	let rows = decisions
		.iter()
		.map(|decision| {
			let package = decision["package"]
				.as_str()
				.unwrap_or_else(|| panic!("decision missing package: {decision}"));
			let short = package
				.replace("cargo:crates/", "")
				.replace("crates/", "")
				.replace("/Cargo.toml", "")
				.replace("/package.json", "")
				.replace("packages/", "")
				.replace(':'.to_string().as_str(), "/")
				.trim_end_matches('/')
				.trim_start_matches('/')
				.to_string();
			serde_json::json!({
				"package": short,
				"bump": decision["bump"],
				"trigger": decision["trigger"],
				"planned_version": decision["planned_version"],
				"upstream_sources": decision["upstream_sources"],
			})
		})
		.collect::<Vec<_>>();
	serde_json::json!({ "decisions": rows })
}

#[test]
fn breaking_source_follows_clamped_inheritance_and_floors() {
	let tempdir = setup_workspace("bump-propagation");
	let json = mc_release(tempdir.path(), &[]);

	assert_json_snapshot!(plan_decisions(&json));
}

#[test]
fn group_declaration_applies_when_the_package_declares_none() {
	let tempdir = setup_workspace("bump-propagation-group");
	let json = mc_release(tempdir.path(), &[]);

	assert_json_snapshot!(plan_decisions(&json));
}

#[test]
fn defaults_bump_propagation_applies_without_declarations() {
	let tempdir = setup_workspace("bump-propagation-defaults");
	let json = mc_release(tempdir.path(), &[]);

	assert_json_snapshot!(plan_decisions(&json));
}
