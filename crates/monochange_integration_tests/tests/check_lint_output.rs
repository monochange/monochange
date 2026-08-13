//! Integration tests for `monochange check` lint output.

use std::ffi::OsString;
use std::path::Path;

use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_fixture(base: &str, name: &str) -> TempDir {
	let source =
		Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/tests/{base}/{name}"));
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	tempdir
}

fn run_check(root: &Path, args: &[&str]) -> String {
	let mut cli_args = vec![OsString::from("monochange"), OsString::from("check")];
	cli_args.extend(args.iter().map(OsString::from));
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));
	let result = runtime.block_on(monochange::run_with_args_in_dir(
		"monochange",
		cli_args,
		root,
	));
	let output = match result {
		Ok(output) => output,
		Err(error) => error.to_string(),
	};
	normalize_workspace_paths(root, output)
}

fn normalize_workspace_paths(root: &Path, output: String) -> String {
	let canonical =
		std::fs::canonicalize(root).unwrap_or_else(|error| panic!("canonicalize root: {error}"));
	let canonical_path = canonical.to_string_lossy();
	let root_path = root.to_string_lossy();
	output
		.replace(canonical_path.as_ref(), "[workspace]")
		.replace(root_path.as_ref(), "[workspace]")
}

#[test]
fn check_lint_output_shows_rule_first_and_verbose_details() {
	let fixture = setup_fixture("check-output", "npm-workspace");
	let output = run_check(fixture.path(), &["--format", "text", "--verbose"]);

	insta::assert_snapshot!(output);
}

#[test]
fn check_sorted_fix_preserves_package_json_field_order() {
	let fixture = setup_fixture("check-output", "npm-workspace");
	std::fs::write(
		fixture.path().join("monochange.toml"),
		r#"[lints]
use = ["npm/recommended"]

[package.app]
path = "packages/app"
type = "npm"
version = "0.0.0"

[package.shared]
path = "packages/shared"
type = "npm"
version = "0.0.0"
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);
	let package_json_path = fixture.path().join("packages/app/package.json");
	let contents = std::fs::read_to_string(&package_json_path)
		.unwrap_or_else(|error| panic!("read package.json: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap_or_else(|error| {
		panic!("package.json should remain valid JSON: {error}\n{contents}")
	});

	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
	insta::assert_snapshot!(contents);
	assert_eq!(parsed["name"], "app");
	assert!(contents.contains(
		r#"    "name": "app",
    "version": "0.0.0",
    "type": "module",
    "description": "Application package",
    "dependencies""#
	));
	assert!(contents.contains(
		r#"    "dependencies": {
        "shared": "^0.0.0",
        "zeta": "1.0.0"
    },"#
	));
}

#[test]
fn check_fix_preserves_package_json_when_multiple_full_file_fixes_exist() {
	let fixture = setup_fixture("check-output", "npm-workspace");
	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);
	let package_json_path = fixture.path().join("packages/app/package.json");
	let contents = std::fs::read_to_string(&package_json_path)
		.unwrap_or_else(|error| panic!("read package.json: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&contents).unwrap_or_else(|error| {
		panic!("package.json should remain valid JSON: {error}\n{contents}")
	});

	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
	insta::assert_snapshot!(contents);
	assert_eq!(parsed["name"], "app");
	assert!(contents.contains(
		r#"    "name": "app",
    "version": "0.0.0",
    "type": "module",
    "description": "Application package",
    "dependencies""#
	));
}
