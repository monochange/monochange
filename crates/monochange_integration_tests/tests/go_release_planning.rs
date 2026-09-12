use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_go_release_fixture(tempdir: &TempDir) {
	let root = tempdir.path();
	copy_directory(&fixture_path("go-release"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
}

fn prepare_release(root: &Path) -> Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.arg("step")
		.arg("prepare-release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run prepare-release: {error}"));
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse prepare-release json: {error}"))
}

fn plan_summary(manifest: &Value) -> Value {
	serde_json::json!({
		"version": manifest["version"],
		"targets": manifest["release_targets"].as_array().map(|targets| {
			targets
				.iter()
				.map(|target| {
					serde_json::json!({
						"id": target["id"],
						"version": target["version"],
						"tagName": target["tag_name"],
						"versionFormat": target["version_format"],
					})
				})
				.collect::<Vec<_>>()
		}),
		"decisions": manifest["plan"]["decisions"].as_array().map(|decisions| {
			decisions
				.iter()
				.map(|decision| {
					serde_json::json!({
						"package": decision["package"],
						"bump": decision["bump"],
						"plannedVersion": decision["planned_version"],
					})
				})
				.collect::<Vec<_>>()
		}),
	})
}

#[test]
fn prepare_release_seeds_go_version_from_release_tag() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_go_release_fixture(&tempdir);
	git(tempdir.path(), &["tag", "v0.1.0"]);

	let manifest = prepare_release(tempdir.path());

	assert_json_snapshot!(plan_summary(&manifest), @r#"
	{
	  "decisions": [
	    {
	      "bump": "patch",
	      "package": "go:go.mod",
	      "plannedVersion": "0.1.1"
	    }
	  ],
	  "targets": [
	    {
	      "id": "api",
	      "tagName": "v0.1.1",
	      "version": "0.1.1",
	      "versionFormat": "primary"
	    }
	  ],
	  "version": "0.1.1"
	}
	"#);
}

#[test]
fn prepare_release_reports_go_packages_without_a_release_baseline() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_go_release_fixture(&tempdir);

	let manifest = prepare_release(tempdir.path());

	assert_json_snapshot!(plan_summary(&manifest), @r#"
	{
	  "decisions": [
	    {
	      "bump": "patch",
	      "package": "go:go.mod",
	      "plannedVersion": null
	    }
	  ],
	  "targets": [],
	  "version": null
	}
	"#);
}
