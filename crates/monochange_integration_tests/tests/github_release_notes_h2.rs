use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_snapshot;
use monochange_config::load_workspace_configuration;
use monochange_core::ReleaseManifest;
use monochange_github::build_release_requests;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(case: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/group-release-note-fallback")
		.join(case)
}

fn setup_case(case: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path(case), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "test"]);
	git(root, &["config", "user.email", "test@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	let output = Command::new("git")
		.current_dir(root)
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

fn mc_json(root: &Path, args: &[&str]) -> Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-06")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {}: {error}", args.join(" ")));
	assert!(
		output.status.success(),
		"monochange {} failed\nstdout:\n{}\nstderr:\n{}",
		args.join(" "),
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
		panic!(
			"parse monochange json: {error}\nstdout:\n{}",
			String::from_utf8_lossy(&output.stdout)
		)
	})
}

fn release_requests(root: &Path) -> Vec<Value> {
	let prepare_output = mc_json(root, &["step", "prepare-release", "--format", "json"]);
	let manifest: ReleaseManifest = serde_json::from_value(prepare_output)
		.unwrap_or_else(|error| panic!("parse release manifest: {error}"));
	let configuration = load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("load workspace configuration: {error}"));
	let source = configuration
		.source
		.as_ref()
		.unwrap_or_else(|| panic!("fixture did not configure a source"));
	build_release_requests(source, &manifest)
		.into_iter()
		.map(|release| {
			serde_json::json!({
				"targetId": release.target_id,
				"tag_name": release.tag_name,
				"name": release.name,
				"body": release.body.unwrap_or_default(),
			})
		})
		.collect()
}

fn normalize_commit_links(contents: &str) -> String {
	contents
		.lines()
		.map(|line| {
			if line.contains("_Introduced in:_ [`") {
				"_Owner:_ test · _Introduced in:_ [`[commit]`](https://github.com/ifiokjr/monochange/commit/[commit])".to_string()
			} else {
				line.to_string()
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

#[test]
fn github_release_notes_use_h2_sections_without_title_in_body() {
	let tempdir = setup_case("group-real-member-note");
	let releases = release_requests(tempdir.path());
	let release = releases
		.first()
		.unwrap_or_else(|| panic!("expected one release: {releases:#?}"));
	let name = release["name"]
		.as_str()
		.unwrap_or_else(|| panic!("release name was not a string: {release:#?}"));
	let body = release["body"]
		.as_str()
		.unwrap_or_else(|| panic!("release body was not a string: {release:#?}"));
	let body = normalize_commit_links(body);

	// Release title carries the version with the date by default, knope-style.
	assert!(
		name.contains("(2026-04-06)"),
		"release name should include date: {name}"
	);
	// Body drops the version title header; sections are h2 like knope.
	assert!(
		!body.contains("## [1.1.0]"),
		"body should not contain version title:\n{body}"
	);
	assert!(body.contains("## Features"), "body:\n{body}");
	assert!(
		!body.lines().any(|line| line.starts_with("### Features")),
		"sections should be h2, not h3:\n{body}"
	);

	assert_snapshot!("group_real_member_note__body", body);
	assert_snapshot!("group_real_member_note__name", name.to_string());
}

#[test]
fn github_grouped_fallback_lists_member_packages_without_version_title() {
	let tempdir = setup_case("group-empty-one-member-note");
	let releases = release_requests(tempdir.path());
	let release = releases
		.first()
		.unwrap_or_else(|| panic!("expected one release: {releases:#?}"));
	let name = release["name"]
		.as_str()
		.unwrap_or_else(|| panic!("release name was not a string: {release:#?}"));
	let body = release["body"]
		.as_str()
		.unwrap_or_else(|| panic!("release body was not a string: {release:#?}"));
	let body = normalize_commit_links(body);

	assert!(
		name.contains("(2026-04-06)"),
		"release name should include date: {name}"
	);
	assert!(
		body.starts_with("Grouped release for `sdk`."),
		"grouped body should start with summary, not title:\n{body}"
	);
	assert!(
		!body.contains("## [1.1.0]"),
		"body should not contain version title:\n{body}"
	);
	assert!(body.contains("## `core`"), "body:\n{body}");
	assert!(body.contains("### Features"), "body:\n{body}");

	assert_snapshot!("group_empty_one_member_note__body", body);
}
