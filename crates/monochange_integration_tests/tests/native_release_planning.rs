use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn setup_native_release_fixture(tempdir: &TempDir) {
	let root = tempdir.path();
	copy_directory(&fixture_path("native-release"), root);
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

fn plan_summary(manifest: &Value) -> Value {
	serde_json::json!({
		"version": manifest["version"],
		"targets": manifest["release_targets"].as_array().map(|targets| {
			targets
				.iter()
				.map(|target| {
					serde_json::json!({
						"id": target["id"],
						"version": target["version"],
						"tagName": target["tag_name"],
						"versionFormat": target["version_format"],
					})
				})
				.collect::<Vec<_>>()
		}),
		"decisions": manifest["plan"]["decisions"].as_array().map(|decisions| {
			decisions
				.iter()
				.map(|decision| {
					serde_json::json!({
						"package": decision["package"],
						"bump": decision["bump"],
						"plannedVersion": decision["planned_version"],
					})
				})
				.collect::<Vec<_>>()
		}),
		"groups": manifest["plan"]["groups"].as_array().map(|groups| {
			groups
				.iter()
				.map(|group| {
					serde_json::json!({
						"group": group["group"],
						"version": group["version"],
						"members": group["members"],
					})
				})
				.collect::<Vec<_>>()
		}),
	})
}

#[test]
fn native_release_plans_swift_cocoapods_and_gradle_packages_together() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_native_release_fixture(&tempdir);
	let root = tempdir.path();

	let manifest = prepare_release(root);

	assert_json_snapshot!("native_release_plan", plan_summary(&manifest));

	// The typed versioned files carry the shared group version into the
	// podspec declaration and the Gradle properties.
	let podspec = fs::read_to_string(root.join("pod/Canvist.podspec"))
		.unwrap_or_else(|error| panic!("read podspec: {error}"));
	assert!(podspec.contains("s.version = \"0.1.1\""), "{podspec}");

	let properties = fs::read_to_string(root.join("android/gradle.properties"))
		.unwrap_or_else(|error| panic!("read gradle.properties: {error}"));
	assert!(properties.contains("VERSION_NAME=0.1.1"), "{properties}");
}

#[test]
fn native_release_discovers_all_three_ecosystems() {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	setup_native_release_fixture(&tempdir);
	let root = tempdir.path();

	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.arg("step")
		.arg("discover")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run discover: {error}"));
	assert!(
		output.status.success(),
		"discover failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);

	let manifest: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse discover json: {error}"));
	let packages = manifest["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("discover packages: {manifest}"));
	let ecosystems = packages
		.iter()
		.map(|package| {
			package["ecosystem"]
				.as_str()
				.unwrap_or_else(|| panic!("package ecosystem: {package}"))
				.to_string()
		})
		.collect::<Vec<_>>();

	assert!(ecosystems.contains(&"swift".to_string()), "{ecosystems:?}");
	assert!(
		ecosystems.contains(&"cocoapods".to_string()),
		"{ecosystems:?}"
	);
	assert!(ecosystems.contains(&"gradle".to_string()), "{ecosystems:?}");
}
