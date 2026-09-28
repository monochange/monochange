//! Integration tests for internal Go dependency rewrites.
//!
//! Go modules carry nested module paths (`github.com/acme/core`), so both the
//! release-preparation flow and `monochange versions sync` must resolve a
//! `require` module path to a workspace package before rewriting it. These
//! tests pin the behavior for a two-module workspace where `service` requires
//! `core` through its full module path and pins it back with a `replace`.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange::sync_workspace_versions;
use monochange_core::VersionStrategy;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;
use tempfile::tempdir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_internal_deps_fixture(tempdir: &TempDir) {
	let root = tempdir.path();
	copy_directory(&fixture_path("go/internal-deps"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
}

fn setup_unrelated_modules_fixture(tempdir: &TempDir) {
	let root = tempdir.path();
	copy_directory(&fixture_path("go/internal-deps-unrelated"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);
}

fn prepare_release(root: &Path) -> Value {
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("MONOCHANGE_RELEASE_DATE", "2026-04-07")
		.arg("step")
		.arg("prepare-release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run prepare-release: {error}"));
	assert!(
		output.status.success(),
		"prepare-release failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse prepare-release json: {error}"))
}

#[test]
fn prepare_release_rewrites_internal_go_require_directives() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_internal_deps_fixture(&tempdir);
	git(tempdir.path(), &["tag", "core/v1.2.0"]);
	git(tempdir.path(), &["tag", "service/v1.0.0"]);

	let manifest = prepare_release(tempdir.path());

	let changed_go_mods = manifest["changed_files"]
		.as_array()
		.map(|files| {
			files
				.iter()
				.filter_map(|path| path.as_str())
				.filter(|path| path.ends_with(".mod"))
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let targets = manifest["release_targets"]
		.as_array()
		.map(|targets| {
			targets
				.iter()
				.map(|target| {
					serde_json::json!({
						"id": target["id"],
						"version": target["version"],
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let summary = serde_json::json!({
		"changed_files": changed_go_mods,
		"targets": targets,
	});
	assert_json_snapshot!(summary, @r#"
	{
	  "changed_files": [
	    "service/go.mod"
	  ],
	  "targets": [
	    {
	      "id": "core",
	      "version": "1.3.0"
	    },
	    {
	      "id": "service",
	      "version": "1.0.1"
	    }
	  ]
	}
	"#);

	let service_go_mod = std::fs::read_to_string(tempdir.path().join("service/go.mod"))
		.unwrap_or_else(|error| panic!("read service/go.mod: {error}"));
	assert_snapshot!(service_go_mod, @r#"
	module github.com/acme/service

	go 1.22

	require github.com/acme/core v1.3.0

	replace github.com/acme/core => ../core
	"#);
}

#[test]
fn versions_sync_reports_pending_change_for_nested_go_module_path() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_internal_deps_fixture(&tempdir);
	// Simulate a released `core`: the `core/v1.3.0` tag exists while
	// `service/go.mod` still requires the stale `v1.2.0`.
	git(tempdir.path(), &["tag", "core/v1.3.0"]);
	git(tempdir.path(), &["tag", "service/v1.0.0"]);

	let mut result = sync_workspace_versions(tempdir.path(), VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("sync_workspace_versions: {error}"));

	// Sync plan paths are absolute; redact the workspace prefix the same way the
	// `versions sync` CLI suite does.
	let canonical = std::fs::canonicalize(tempdir.path())
		.unwrap_or_else(|error| panic!("canonicalize root: {error}"))
		.to_string_lossy()
		.to_string();
	for file in &mut result.changes {
		file.path = file.path.replace(canonical.as_str(), "[workspace]");
	}

	assert_json_snapshot!(result, @r#"
	{
	  "applied": false,
	  "strategy": "Default",
	  "changes": [
	    {
	      "path": "[workspace]/service/go.mod",
	      "ecosystem": "go",
	      "changes": [
	        {
	          "dependency_name": "github.com/acme/core",
	          "section": "require",
	          "old_value": "v1.2.0",
	          "new_value": "v1.3.0"
	        }
	      ]
	    }
	  ],
	  "skipped": []
	}
	"#);
}

#[test]
fn prepare_release_leaves_unrelated_modules_sharing_a_last_segment_untouched() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_unrelated_modules_fixture(&tempdir);
	git(tempdir.path(), &["tag", "core/v1.2.0"]);
	git(tempdir.path(), &["tag", "sdk/v2.0.0"]);
	git(tempdir.path(), &["tag", "service/v1.0.0"]);
	git(tempdir.path(), &["tag", "tools/v0.4.0"]);

	let manifest = prepare_release(tempdir.path());

	let changed_go_mods = manifest["changed_files"]
		.as_array()
		.map(|files| {
			files
				.iter()
				.filter_map(|path| path.as_str())
				.filter(|path| path.ends_with(".mod"))
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let targets = manifest["release_targets"]
		.as_array()
		.map(|targets| {
			targets
				.iter()
				.map(|target| {
					serde_json::json!({
						"id": target["id"],
						"version": target["version"],
					})
				})
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();
	let summary = serde_json::json!({
		"changed_files": changed_go_mods,
		"targets": targets,
	});
	assert_json_snapshot!(summary, @r#"
	{
	  "changed_files": [
	    "service/go.mod",
	    "tools/go.mod"
	  ],
	  "targets": [
	    {
	      "id": "core",
	      "version": "1.3.0"
	    },
	    {
	      "id": "sdk",
	      "version": "2.1.0"
	    },
	    {
	      "id": "service",
	      "version": "1.0.1"
	    },
	    {
	      "id": "tools",
	      "version": "0.4.1"
	    }
	  ]
	}
	"#);

	let service_go_mod = std::fs::read_to_string(tempdir.path().join("service/go.mod"))
		.unwrap_or_else(|error| panic!("read service/go.mod: {error}"));
	assert_snapshot!(service_go_mod, @r#"
	module github.com/acme/service

	go 1.22

	require (
		github.com/acme/core v1.3.0
		github.com/other/core v0.9.0
		github.com/acme/sdk/v2 v2.1.0
		github.com/other/sdk/v2 v0.1.0
	)

	replace github.com/acme/core => ../core
	"#);

	let tools_go_mod = std::fs::read_to_string(tempdir.path().join("tools/go.mod"))
		.unwrap_or_else(|error| panic!("read tools/go.mod: {error}"));
	assert_snapshot!(tools_go_mod, @r"
	module github.com/acme/tools

	go 1.22

	require github.com/acme/core v1.3.0

	require github.com/other/core v0.9.0
	");
}

#[test]
fn versions_sync_leaves_unrelated_modules_sharing_a_last_segment_untouched() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_unrelated_modules_fixture(&tempdir);
	// Simulate released `core` and `sdk/v2`: higher tags exist while
	// `service/go.mod` still requires stale versions.
	git(tempdir.path(), &["tag", "core/v1.3.0"]);
	git(tempdir.path(), &["tag", "sdk/v2.1.0"]);
	git(tempdir.path(), &["tag", "service/v1.0.0"]);
	git(tempdir.path(), &["tag", "tools/v0.4.0"]);

	let mut result = sync_workspace_versions(tempdir.path(), VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("sync_workspace_versions: {error}"));

	// Sync plan paths are absolute; redact the workspace prefix the same way the
	// `versions sync` CLI suite does.
	let canonical = std::fs::canonicalize(tempdir.path())
		.unwrap_or_else(|error| panic!("canonicalize root: {error}"))
		.to_string_lossy()
		.to_string();
	for file in &mut result.changes {
		file.path = file.path.replace(canonical.as_str(), "[workspace]");
	}

	assert_json_snapshot!(result, @r#"
	{
	  "applied": false,
	  "strategy": "Default",
	  "changes": [
	    {
	      "path": "[workspace]/service/go.mod",
	      "ecosystem": "go",
	      "changes": [
	        {
	          "dependency_name": "github.com/acme/core",
	          "section": "require",
	          "old_value": "v1.2.0",
	          "new_value": "v1.3.0"
	        },
	        {
	          "dependency_name": "github.com/acme/sdk/v2",
	          "section": "require",
	          "old_value": "v2.0.0",
	          "new_value": "v2.1.0"
	        }
	      ]
	    },
	    {
	      "path": "[workspace]/tools/go.mod",
	      "ecosystem": "go",
	      "changes": [
	        {
	          "dependency_name": "github.com/acme/core",
	          "section": "require",
	          "old_value": "v1.2.0",
	          "new_value": "v1.3.0"
	        }
	      ]
	    }
	  ],
	  "skipped": []
	}
	"#);
}
