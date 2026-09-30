//! Custom tag formats identify the same release baseline in planning and analysis.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_test_helpers::git::git;
use monochange_test_helpers::setup_fixture;
use serde_json::Value;

#[test]
fn custom_go_tags_resolve_baseline_next_version_and_previous_title() {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("build test runtime: {error}"));
	runtime.block_on(async {
		let fixture = setup_fixture!("go/custom-tag-baseline/workspace");
		let root = fixture.path();
		let original = fs::read(root.join("go.mod"))
			.unwrap_or_else(|error| panic!("read original Go manifest: {error}"));
		git(root, &["init", "-b", "main"]);
		git(root, &["config", "user.name", "monochange-tests"]);
		git(
			root,
			&["config", "user.email", "monochange-tests@example.com"],
		);
		git(root, &["add", "."]);
		git(root, &["commit", "-m", "initial fixture"]);
		let tag_fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/go/custom-tag-baseline/tags.txt");
		let tags = fs::read_to_string(tag_fixture)
			.unwrap_or_else(|error| panic!("read tag fixture: {error}"));
		for tag in tags.lines() {
			git(root, &["tag", tag]);
		}

		run_cli(root, &["step", "validate"]).await;
		let preview = run_cli(root, &["next", "--format", "json"]).await;
		let preview: Value = serde_json::from_str(&preview)
			.unwrap_or_else(|error| panic!("parse release preview: {error}"));
		insta::assert_json_snapshot!(preview);
		let release = run_cli(
			root,
			&["step", "prepare-release", "--dry-run", "--format", "json"],
		)
		.await;
		let release: Value = serde_json::from_str(&release)
			.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
		assert_eq!(release["release_targets"][0]["version"], "1.11.0");
		assert_eq!(
			release["release_targets"][0]["tag_name"],
			"go/module_owner/release-1.11.0-ready"
		);
		assert_eq!(
			release["release_targets"][0]["rendered_title"],
			"1.10.0 -> 1.11.0"
		);
		assert_eq!(
			release["release_targets"][0]["rendered_changelog_title"],
			"1.10.0 -> 1.11.0"
		);
		insta::assert_json_snapshot!(release["release_targets"]);
		assert_eq!(
			fs::read(root.join("go.mod"))
				.unwrap_or_else(|error| panic!("read untouched Go manifest: {error}")),
			original
		);
		let analysis = run_cli(
			root,
			&[
				"analyze",
				"--package",
				"module_owner",
				"--main-ref",
				"main",
				"--format",
				"json",
			],
		)
		.await;
		let analysis: Value = serde_json::from_str(&analysis)
			.unwrap_or_else(|error| panic!("parse analysis: {error}"));
		assert_eq!(
			analysis["refs"]["release"],
			"go/module_owner/release-1.10.0-ready"
		);
		insta::assert_json_snapshot!(analysis["refs"]);
	});
}

async fn run_cli(root: &Path, args: &[&str]) -> String {
	let args = std::iter::once("monochange")
		.chain(args.iter().copied())
		.map(OsString::from)
		.collect::<Vec<_>>();
	monochange::run_with_args_in_dir("monochange", args, root)
		.await
		.unwrap_or_else(|error| panic!("monochange command: {error}"))
}
