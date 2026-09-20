use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use monochange_test_helpers::git::git_output;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_actions_fixture(tempdir: &TempDir) {
	let root = tempdir.path();
	copy_directory(&fixture_path("github-actions-release"), root);
	git(root, &["init", "--initial-branch", "main"]);
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

fn release_summary(manifest: &Value) -> Value {
	serde_json::json!({
		"version": manifest["version"],
		"targets": manifest["release_targets"].as_array().map(|targets| {
			targets
				.iter()
				.map(|target| {
					serde_json::json!({
						"id": target["id"],
						"kind": target["kind"],
						"version": target["version"],
						"tagName": target["tag_name"],
						"tag": target["tag"],
						"release": target["release"],
						"floatingTags": target["floating_tags"],
					})
				})
				.collect::<Vec<_>>()
		}),
		"packagePublications": manifest["package_publications"],
		"changedFiles": manifest["changed_files"],
	})
}

#[test]
fn prepare_release_plans_github_actions_release_from_tag() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_actions_fixture(&tempdir);
	git(tempdir.path(), &["tag", "v1.0.0"]);

	let manifest = prepare_release(tempdir.path());

	assert_json_snapshot!(release_summary(&manifest), @r#"
	{
	  "changedFiles": [
	    "package.json"
	  ],
	  "packagePublications": [],
	  "targets": [
	    {
	      "floatingTags": [
	        "v{{ major }}.{{ minor }}",
	        "v{{ major }}"
	      ],
	      "id": "actions",
	      "kind": "package",
	      "release": true,
	      "tag": true,
	      "tagName": "v1.0.1",
	      "version": "1.0.1"
	    }
	  ],
	  "version": "1.0.1"
	}
	"#);
}

#[test]
fn tag_release_moves_floating_tags_for_github_actions() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_actions_fixture(&tempdir);
	git(tempdir.path(), &["tag", "v1.0.0"]);

	prepare_release(tempdir.path());
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(tempdir.path())
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.arg("step")
		.arg("commit-release")
		.output()
		.unwrap_or_else(|error| panic!("run commit-release: {error}"));
	assert!(
		output.status.success(),
		"commit-release failed\nstderr:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(tempdir.path())
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.arg("step")
		.arg("tag-release")
		.arg("--from")
		.arg("HEAD")
		.arg("--push=false")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run tag-release: {error}"));
	assert!(
		output.status.success(),
		"tag-release failed\nstderr:\n{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let report: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse tag-release json: {error}"));

	let tags = git_output(tempdir.path(), &["tag", "--list"]);
	let commit = git_output(tempdir.path(), &["rev-parse", "HEAD"]);
	for tag in ["v1.0.1", "v1.0", "v1"] {
		assert!(
			tags.lines().any(|line| line.trim() == tag),
			"expected tag {tag} in {tags:?}"
		);
		assert_eq!(
			git_output(tempdir.path(), &["rev-parse", &format!("{tag}^{{}}")]),
			commit,
			"tag {tag} should point at the release commit"
		);
	}

	assert_json_snapshot!(report["tag_results"][0]["tag_name"], @r#""v1.0.1""#);
}
