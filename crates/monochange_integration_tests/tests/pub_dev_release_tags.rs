//! Integration coverage for publishing pub.dev packages from their own
//! release tag.
//!
//! pub.dev trusted publishing only accepts an upload from a GitHub Actions run
//! whose ref is the package's release tag. The fixture releases an npm package
//! under the primary `v{{ version }}` tag and a Dart package under its own
//! `app_dart/v{{ version }}` tag, then runs the publish dry run as if GitHub
//! Actions had started it on different refs.

#![allow(clippy::disallowed_methods)]

use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use httpmock::Method::GET;
use httpmock::MockServer;
use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use rstest::rstest;
use serde_json::Value;
use tempfile::TempDir;

const FIXTURE_ROOT: &str = "../../fixtures/tests/pub-dev-release-tags";

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join(FIXTURE_ROOT)
		.join(relative)
}

fn monochange_command(root: &Path) -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-11-25")
		.env_remove("RUST_LOG");
	command
}

fn stdout_of(output: &Output) -> String {
	String::from_utf8(output.stdout.clone())
		.unwrap_or_else(|error| panic!("stdout was not UTF-8: {error}"))
}

fn stderr_of(output: &Output) -> String {
	String::from_utf8(output.stderr.clone())
		.unwrap_or_else(|error| panic!("stderr was not UTF-8: {error}"))
}

/// Copy the workspace fixture and create its release commit, so the publish
/// step finds `app_dart` 0.2.1 and `web` 1.1.0 in the release record at `HEAD`.
fn setup_released_workspace() -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_directory(&fixture_path("workspace"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "test"]);
	git(root, &["config", "user.email", "test@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	let output = monochange_command(root)
		.arg("commit-release")
		.output()
		.unwrap_or_else(|error| panic!("run commit-release: {error}"));
	assert!(
		output.status.success(),
		"commit-release failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);

	tempdir
}

/// `PATH` with the fixture's `dart` and `npm` stand-ins first, so planned
/// packages run a dry-run publish command without the real toolchains.
fn path_with_tool_stubs() -> OsString {
	let mut paths = vec![fixture_path("tools")];
	paths.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
	env::join_paths(paths).unwrap_or_else(|error| panic!("join PATH: {error}"))
}

/// Run the publish dry run as a GitHub Actions run of `publish.yml` on
/// `git_ref`, against registry mocks.
fn publish_dry_run(root: &Path, server: &MockServer, git_ref: &str, format: &str) -> Output {
	monochange_command(root)
		.env("PATH", path_with_tool_stubs())
		.env("GITHUB_ACTIONS", "true")
		.env("GITHUB_EVENT_NAME", "workflow_dispatch")
		.env("GITHUB_REF", git_ref)
		.env("GITHUB_REPOSITORY", "example/pub-dev-release-tags")
		.env(
			"GITHUB_WORKFLOW_REF",
			format!("example/pub-dev-release-tags/.github/workflows/publish.yml@{git_ref}"),
		)
		.env("MONOCHANGE_NPM_REGISTRY_URL", server.url("/npm"))
		.env("MONOCHANGE_PUB_DEV_API_URL", server.url("/pub"))
		.env_remove("PUB_TOKEN")
		.env_remove("ACTIONS_ID_TOKEN_REQUEST_URL")
		.env_remove("ACTIONS_ID_TOKEN_REQUEST_TOKEN")
		.args(["publish", "packages", "--dry-run", "--format", format])
		.output()
		.unwrap_or_else(|error| panic!("run publish packages: {error}"))
}

#[rstest]
#[case::primary_tag("primary_tag", "refs/tags/v1.1.0", true)]
#[case::package_tag("package_tag", "refs/tags/app_dart/v0.2.1", false)]
#[case::unrelated_tag("unrelated_tag", "refs/tags/v9.9.9", true)]
fn publish_dry_run_scopes_the_pub_dev_package_to_its_release_tag(
	#[case] name: &str,
	#[case] git_ref: &str,
	#[case] npm_in_scope: bool,
) {
	let workspace = setup_released_workspace();
	let server = MockServer::start();
	let npm_lookup = server.mock(|when, then| {
		when.method(GET).path("/npm/web");
		then.status(404);
	});
	let pub_dev_lookup = server.mock(|when, then| {
		when.method(GET).path("/pub/packages/app_dart");
		then.status(200).json_body(
			serde_json::json!({ "name": "app_dart", "versions": [{ "version": "0.2.0" }] }),
		);
	});

	let text = publish_dry_run(workspace.path(), &server, git_ref, "text");
	assert!(
		text.status.success(),
		"publish dry run failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&text),
		stderr_of(&text)
	);
	assert_snapshot!(format!("publish_dry_run_{name}_text"), stdout_of(&text));

	let json = publish_dry_run(workspace.path(), &server, git_ref, "json");
	assert!(
		json.status.success(),
		"publish dry run failed\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&json),
		stderr_of(&json)
	);
	let value: Value = serde_json::from_slice(&json.stdout)
		.unwrap_or_else(|error| panic!("parse publish json: {error}"));
	assert_json_snapshot!(
		format!("publish_dry_run_{name}_json"),
		value["package_publish"]
	);

	// A package that is out of scope for the run never reaches its registry.
	assert_eq!(npm_lookup.calls() > 0, npm_in_scope, "npm lookups");
	assert!(pub_dev_lookup.calls() > 0, "pub.dev lookups");
}

#[test]
fn validate_rejects_trusted_pub_dev_package_without_release_tag() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("missing-tag"), tempdir.path());

	let output = monochange_command(tempdir.path())
		.env_remove("PUB_TOKEN")
		.args(["step", "validate"])
		.output()
		.unwrap_or_else(|error| panic!("run validate: {error}"));

	assert!(
		!output.status.success(),
		"validate should fail\nstdout:\n{}\nstderr:\n{}",
		stdout_of(&output),
		stderr_of(&output)
	);
	assert_snapshot!("validate_missing_release_tag", stderr_of(&output));
}
