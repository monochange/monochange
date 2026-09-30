//! Deno JSONC packages must remain usable after generating their configuration.

use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_test_helpers::setup_fixture;
use serde_json::Value;

#[tokio::test]
#[expect(
	clippy::disallowed_methods,
	reason = "tokio::test builds its runtime with Result::expect"
)]
async fn init_supports_deno_jsonc_through_release_preparation() {
	let fixture = setup_fixture!("deno/jsonc-init/workspace");
	let root = fixture.path();
	let original = fs::read_to_string(root.join("deno.jsonc"))
		.unwrap_or_else(|error| panic!("read original manifest: {error}"));

	run_cli(root, &["init"]).await;
	run_cli(root, &["step", "validate"]).await;
	run_cli(
		root,
		&[
			"create",
			"--package",
			"@acme/commented",
			"--bump",
			"patch",
			"--reason",
			"Preserve comments when preparing a Deno release",
		],
	)
	.await;
	run_cli(root, &["step", "validate"]).await;
	let preview = run_cli(root, &["next", "--format", "json"]).await;
	let preview: Value = serde_json::from_str(&preview)
		.unwrap_or_else(|error| panic!("parse next version: {error}"));
	insta::assert_json_snapshot!(preview);
	let dry_run = run_cli(
		root,
		&["step", "prepare-release", "--dry-run", "--format", "json"],
	)
	.await;
	let dry_run: Value = serde_json::from_str(&dry_run)
		.unwrap_or_else(|error| panic!("parse dry-run release: {error}"));
	insta::assert_json_snapshot!(dry_run["plan"]);
	let unchanged = fs::read_to_string(root.join("deno.jsonc"))
		.unwrap_or_else(|error| panic!("read dry-run manifest: {error}"));
	assert_eq!(unchanged, original);

	let prepared = run_cli(root, &["step", "prepare-release", "--format", "json"]).await;
	let prepared: Value = serde_json::from_str(&prepared)
		.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
	assert_eq!(prepared["plan"], dry_run["plan"]);
	let manifest = fs::read_to_string(root.join("deno.jsonc"))
		.unwrap_or_else(|error| panic!("read prepared manifest: {error}"));
	insta::assert_snapshot!(manifest);
	assert!(!root.join("deno.json").exists());
	run_cli(root, &["step", "validate"]).await;
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
