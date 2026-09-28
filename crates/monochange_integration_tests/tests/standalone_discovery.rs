//! Integration tests for discovery across several standalone packages.
//!
//! A repository can contain many independent packages without a workspace root
//! manifest between them. Discovery used to parse every standalone manifest with
//! its own directory as the workspace root, so all of them produced the id
//! `<ecosystem>:<manifest-file>` and the sort+dedup kept only one package per
//! ecosystem. These tests pin the public `step discover` output so a regression
//! is visible from the CLI surface, not just from adapter unit tests.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::Path;

use insta::assert_json_snapshot;
use monochange_test_helpers::setup_fixture;
use serde_json::Value;

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn step_discover_keeps_every_standalone_package_without_a_workspace_root() {
	let fixture = setup_fixture!("standalone-discovery/no-workspace-root");
	let output = run_discover(fixture.path()).await;
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("discover output was not JSON: {error}\n{output}"));

	let package_ids = value["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages was not an array: {value:#?}"))
		.iter()
		.filter_map(|package| package["id"].as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		package_ids,
		vec![
			"cargo:crates/alpha/Cargo.toml",
			"cargo:crates/beta/Cargo.toml",
			"cargo:crates/gamma/Cargo.toml",
			"dart:dart/mobile_sdk/pubspec.yaml",
			"dart:dart/mobile_ui/pubspec.yaml",
			"deno:deno/helper/deno.json",
			"deno:deno/tool/deno.json",
			"python:python/cli/pyproject.toml",
			"python:python/core/pyproject.toml",
		]
	);
	assert_eq!(value["warnings"], serde_json::json!([]));

	// Every standalone package keeps ids relative to the discovery root, so the
	// payload stays stable no matter which manifest is visited first.
	assert_eq!(value["workspace_root"], serde_json::json!("."));
	assert_json_snapshot!(discovery_summary(&value));
}

#[tokio::test]
#[allow(clippy::disallowed_methods)]
async fn versions_list_reports_every_standalone_package() {
	let fixture = setup_fixture!("standalone-discovery/no-workspace-root");
	let output = monochange::run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("list"),
			OsString::from("--format"),
			OsString::from("json"),
		],
		fixture.path(),
	)
	.await
	.unwrap_or_else(|error| panic!("versions list failed: {error}"));
	let value: Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("versions output was not JSON: {error}\n{output}"));

	// The inventory payload is a flat map of release identity to version.
	let package_versions = value
		.as_object()
		.unwrap_or_else(|| panic!("versions inventory was not an object: {value:#?}"));
	let package_names = package_versions
		.keys()
		.map(String::as_str)
		.collect::<BTreeSet<_>>();
	assert_eq!(
		package_names,
		[
			"alpha",
			"beta",
			"gamma",
			"deno-helper",
			"deno-tool",
			"mobile-sdk",
			"mobile-ui",
			"py-cli",
			"py-core",
		]
		.into_iter()
		.collect::<BTreeSet<_>>()
	);
	assert_eq!(package_versions["py-core"], serde_json::json!("1.0.0"));
	assert_eq!(package_versions["py-cli"], serde_json::json!("4.0.0"));
	assert_json_snapshot!(value);
}

async fn run_discover(root: &Path) -> String {
	monochange::run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("discover"),
			OsString::from("--format"),
			OsString::from("json"),
		],
		root,
	)
	.await
	.unwrap_or_else(|error| panic!("discover command failed: {error}"))
}

/// Reduce the discovery payload to the fields this regression cares about.
fn discovery_summary(value: &Value) -> Value {
	serde_json::json!({
		"package_ids": value["packages"]
			.as_array()
			.unwrap_or_else(|| panic!("packages was not an array: {value:#?}"))
			.iter()
			.filter_map(|package| package["id"].as_str())
			.collect::<Vec<_>>(),
		"warnings": value["warnings"],
	})
}
