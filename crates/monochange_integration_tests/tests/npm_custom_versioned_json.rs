//! Typed JSON metadata follows the same contract during validation and preparation.

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
async fn custom_npm_json_files_validate_and_prepare() {
	for scenario in ["custom-versioned-json", "custom-versioned-json-glob"] {
		let fixture = setup_fixture!(&format!("npm/{scenario}/workspace"));
		let root = fixture.path();
		let constraints_path = root.join("packages/ui/constraints.json");
		let original = fs::read_to_string(&constraints_path)
			.unwrap_or_else(|error| panic!("read original constraints: {error}"));

		run_cli(root, &["step", "validate"]).await;
		let dry_run = run_cli(root, &["prepare", "--dry-run", "--format", "json"]).await;
		let dry_run: Value = serde_json::from_str(&dry_run)
			.unwrap_or_else(|error| panic!("parse dry-run release: {error}"));
		let unchanged = fs::read_to_string(&constraints_path)
			.unwrap_or_else(|error| panic!("read dry-run constraints: {error}"));
		assert_eq!(unchanged, original);

		let prepared = run_cli(root, &["prepare", "--format", "json"]).await;
		let prepared: Value = serde_json::from_str(&prepared)
			.unwrap_or_else(|error| panic!("parse prepared release: {error}"));
		assert_eq!(prepared["plan"], dry_run["plan"]);
		let constraints = fs::read_to_string(&constraints_path)
			.unwrap_or_else(|error| panic!("read prepared constraints: {error}"));
		insta::assert_snapshot!(scenario, constraints);
		run_cli(root, &["step", "validate"]).await;
	}
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
