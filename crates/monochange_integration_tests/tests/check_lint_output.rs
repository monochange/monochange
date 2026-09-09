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
		Err(error) => {
			error
				.reported_output()
				.map_or_else(|| error.to_string(), ToOwned::to_owned)
		}
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

#[test]
fn check_reports_redundant_changeset_entries_without_fix() {
	let fixture = setup_fixture("check-output", "changeset-prefer-inline");
	let output = run_check(fixture.path(), &["--format", "text", "--verbose"]);

	insta::assert_snapshot!(output);
}

#[test]
fn check_fix_converts_redundant_changeset_entries_to_inline() {
	let fixture = setup_fixture("check-output", "changeset-prefer-inline");
	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);

	let block_changeset =
		std::fs::read_to_string(fixture.path().join(".changeset/redundant-object.md"))
			.unwrap_or_else(|error| panic!("read redundant-object.md: {error}"));
	let flow_changeset =
		std::fs::read_to_string(fixture.path().join(".changeset/redundant-flow.md"))
			.unwrap_or_else(|error| panic!("read redundant-flow.md: {error}"));

	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
	insta::assert_snapshot!(block_changeset);
	insta::assert_snapshot!(flow_changeset);
}

/// Regression test for the `cargo/manifest-repository` autofix: enabling the
/// rule with `allow_workspace_inheritance = true` must leave manifests that
/// use `repository = { workspace = true }` completely untouched, and must
/// never replace a manifest with a fragment of itself.
#[test]
fn check_fix_leaves_inherited_repositories_alone_when_opted_in() {
	let fixture = setup_fixture("check-output", "cargo-manifest-repository");
	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);

	let member_manifest = fixture.path().join("crates/example/Cargo.toml");
	let contents = std::fs::read_to_string(&member_manifest)
		.unwrap_or_else(|error| panic!("read member manifest: {error}"));
	assert_eq!(
		contents,
		"[package]\nname = \"example\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT\"\ndescription = \"Example package\"\nrepository = { workspace = true }\n\n[dependencies]\nserde = { version = \"1.0\", features = [\"derive\"] }\n",
		"the manifest must remain byte-identical when workspace inheritance is allowed"
	);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
}

/// The rule without the opt-out resolves the inherited value and rewrites the
/// manifest to the canonical subdirectory URL while keeping every other field.
#[test]
fn check_fix_resolves_inherited_repository_to_subdirectory_url() {
	let fixture = setup_fixture("check-output", "cargo-manifest-repository");
	std::fs::write(
		fixture.path().join("monochange.toml"),
		r#"[defaults]
package_type = "cargo"
changelog = false

[source]
provider = "github"
owner = "acme"
repo = "widgets"

[lints.rules]
"cargo/manifest-repository" = "error"

[ecosystems.cargo]
enabled = true

[package.example]
path = "crates/example"
type = "cargo"
version = "0.1.0"
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let output = run_check(fixture.path(), &["--format", "text", "--fix"]);

	let member_manifest = fixture.path().join("crates/example/Cargo.toml");
	let contents = std::fs::read_to_string(&member_manifest)
		.unwrap_or_else(|error| panic!("read member manifest: {error}"));
	assert!(
		contents
			.contains("repository = \"https://github.com/acme/widgets/tree/main/crates/example\""),
		"repository should be rewritten to the subdirectory URL:\n{contents}"
	);
	// The rewrite must keep all unrelated content intact.
	assert!(
		contents.contains("name = \"example\""),
		"lost name:\n{contents}"
	);
	assert!(
		contents.contains("version = \"0.1.0\""),
		"lost version:\n{contents}"
	);
	assert!(
		contents.contains("edition = \"2021\""),
		"lost edition:\n{contents}"
	);
	assert!(
		contents.contains("license = \"MIT\""),
		"lost license:\n{contents}"
	);
	assert!(
		contents.contains("description = \"Example package\""),
		"lost description:\n{contents}"
	);
	assert!(
		contents.contains("serde = { version = \"1.0\", features = [\"derive\"] }"),
		"lost dependencies:\n{contents}"
	);
	insta::assert_snapshot!(normalize_workspace_paths(fixture.path(), output));
	insta::assert_snapshot!(contents);
}
