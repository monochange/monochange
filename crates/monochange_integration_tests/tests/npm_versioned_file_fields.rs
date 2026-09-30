//! Explicit versioned-file fields and prefixes survive release preparation.

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
async fn prepare_preserves_explicit_npm_versioned_file_fields_and_prefixes() {
	let fixture = setup_fixture!("npm/versioned-file-prefix-precedence/workspace");
	let root = fixture.path();
	let original = fs::read_to_string(root.join("packages/ui/package.json"))
		.unwrap_or_else(|error| panic!("read original manifest: {error}"));

	run_cli(root, &["step", "validate"]).await;
	let dry_run = run_cli(
		root,
		&["step", "prepare-release", "--dry-run", "--format", "json"],
	)
	.await;
	let dry_run: Value = serde_json::from_str(&dry_run)
		.unwrap_or_else(|error| panic!("parse dry-run release: {error}"));
	let unchanged = fs::read_to_string(root.join("packages/ui/package.json"))
		.unwrap_or_else(|error| panic!("read dry-run manifest: {error}"));
	assert_eq!(unchanged, original);
	assert!(!root.join("lockfile-input.json").exists());

	let prepared = run_cli(root, &["step", "prepare-release", "--format", "json"]).await;
	let prepared: Value = serde_json::from_str(&prepared)
		.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
	assert_eq!(prepared["plan"], dry_run["plan"]);
	let manifest = fs::read_to_string(root.join("packages/ui/package.json"))
		.unwrap_or_else(|error| panic!("read prepared manifest: {error}"));
	let manifest_value: Value = serde_json::from_str(&manifest)
		.unwrap_or_else(|error| panic!("parse prepared manifest: {error}"));
	assert_eq!(manifest_value["dependencies"]["@acme/api"], "=2.4.0");
	let lockfile_input = fs::read_to_string(root.join("lockfile-input.json"))
		.unwrap_or_else(|error| panic!("read lockfile command input: {error}"));
	assert_eq!(lockfile_input, manifest);
	insta::assert_snapshot!(manifest);
	let constraints = fs::read_to_string(root.join("packages/ui/constraints/package.json"))
		.unwrap_or_else(|error| panic!("read prepared constraints: {error}"));
	insta::assert_snapshot!(constraints);

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
