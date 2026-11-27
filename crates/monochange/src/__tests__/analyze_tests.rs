#![allow(clippy::disallowed_methods)]
use std::path::Path;

use monochange_core::VersionFormat;
use monochange_core::VersionSource;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::fs::fixture_path_from;
use monochange_test_helpers::git::git;
use tempfile::TempDir;

use super::*;

fn setup_analyze_repo(tag_release: bool, with_origin_head: bool) -> TempDir {
	let scenario_root = fixture_path_from(
		env!("CARGO_MANIFEST_DIR"),
		"cli-output/analyze-group-release-trajectory",
	);
	let release = scenario_root.join("release");
	let main = scenario_root.join("main");
	let head = scenario_root.join("head");
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&release, root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);

	if tag_release {
		git(root, &["tag", "v1.0.0"]);
	}

	if with_origin_head {
		let remote_dir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		git(remote_dir.path(), &["init", "--bare"]);
		let remote = remote_dir.keep();
		git(
			root,
			&["remote", "add", "origin", remote.to_string_lossy().as_ref()],
		);
		git(root, &["push", "-u", "origin", "main"]);
		git(root, &["remote", "set-head", "origin", "main"]);
	}

	copy_directory(&main, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main evolution"]);
	git(root, &["checkout", "-b", "feature"]);
	copy_directory(&head, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "head evolution"]);

	tempdir
}

#[tokio::test(flavor = "multi_thread")]
async fn render_analyze_report_supports_json_with_explicit_refs() {
	let tempdir = setup_analyze_repo(true, true);
	let rendered = render_analyze_report(
		tempdir.path(),
		"core",
		Some("v1.0.0"),
		Some("main"),
		Some("HEAD"),
		"semantic",
		OutputFormat::Json,
	)
	.await
	.unwrap_or_else(|error| panic!("render analyze json: {error}"));

	assert!(rendered.contains("\"release\": \"v1.0.0\""));
	assert!(rendered.contains("\"first_release\": false"));
	assert!(rendered.contains("\"release_to_head\""));
	assert!(rendered.contains("\"item_path\": \"shout\""));
}

#[tokio::test(flavor = "multi_thread")]
async fn render_analyze_report_supports_first_release_fallback_and_text_warnings() {
	let tempdir = setup_analyze_repo(false, false);
	let rendered = render_analyze_report(
		tempdir.path(),
		"core",
		None,
		None,
		None,
		"signature",
		OutputFormat::Text,
	)
	.await
	.unwrap_or_else(|error| panic!("render analyze text: {error}"));

	assert!(rendered.contains("release none · main "), "{rendered}");
	assert!(rendered.contains(" · first release"), "{rendered}");
	assert!(rendered.contains("no prior release tag found for group `sdk`"));
	assert!(rendered.contains("\nmain → head ("), "{rendered}");
}

#[tokio::test(flavor = "multi_thread")]
async fn default_branch_and_release_tag_resolution_cover_origin_head_and_missing_identity_paths() {
	let tempdir = setup_analyze_repo(true, true);
	assert_eq!(
		default_branch_name(tempdir.path())
			.await
			.unwrap_or_else(|error| panic!("default branch: {error}")),
		"main"
	);
	assert_eq!(
		latest_release_tag_for_identity(tempdir.path(), None, "cargo")
			.await
			.unwrap_or_else(|error| panic!("no identity: {error}")),
		None
	);

	let no_tag_identity = EffectiveReleaseIdentity {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		group_id: None,
		tag: false,
		release: false,
		version_format: VersionFormat::Namespaced,
		version_source: VersionSource::default(),
		initial_version: None,
		floating_tags: Vec::new(),
		members: vec!["core".to_string()],
		bump_ceiling: None,
		classification_enforced: true,
	};
	assert_eq!(
		latest_release_tag_for_identity(tempdir.path(), Some(&no_tag_identity), "cargo")
			.await
			.unwrap_or_else(|error| panic!("no tag identity: {error}")),
		None
	);
}
