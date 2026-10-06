//! Private website versioning must not change or publish the CLI.

use std::path::Path;
use std::process::Command;

use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;

#[test]
fn website_release_writes_user_artifacts_and_preserves_cli_version() {
	let workspace = tempfile::TempDir::new().unwrap();
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/website-releases/workspace");
	copy_directory(&fixture, workspace.path());
	let root_before = std::fs::read_to_string(workspace.path().join("Cargo.toml")).unwrap();
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(workspace.path())
		.env("MONOCHANGE_RELEASE_DATE", "2026-10-06")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.args(["step", "prepare-release", "--format", "json"])
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
	assert_eq!(
		std::fs::read_to_string(workspace.path().join("Cargo.toml")).unwrap(),
		root_before
	);
	assert!(
		result["package_publications"]
			.as_array()
			.unwrap()
			.is_empty()
	);
	let artifacts = result["changelogs"].as_array().unwrap().iter().map(|artifact| {
		serde_json::json!({ "owner": artifact["owner_id"], "output": artifact["output"], "stream": artifact["stream"], "path": artifact["path"] })
	}).collect::<Vec<_>>();
	insta::assert_json_snapshot!(
		serde_json::json!({ "targets": result["release_targets"], "artifacts": artifacts })
	);
	let manifest = std::fs::read_to_string(
		workspace
			.path()
			.join("app/crates/monochange_app/Cargo.toml"),
	)
	.unwrap();
	assert!(manifest.contains("version = \"0.1.1\""));
	let notes =
		std::fs::read_to_string(workspace.path().join("app/public/releases/0.1.1.json")).unwrap();
	let notes: serde_json::Value = serde_json::from_str(&notes).unwrap();
	assert_eq!(notes["title"], "0.1.1");
	assert_eq!(notes["sections"][0]["entries"][0]["stream"], "website");
	insta::assert_snapshot!(
		std::fs::read_to_string(workspace.path().join("app/changelog.md")).unwrap()
	);
	assert!(!workspace.path().join(".changeset/website.md").exists());
}
