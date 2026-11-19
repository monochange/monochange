use std::fs;
use std::path::Path;

use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use semver::Version;
use tempfile::tempdir;

use crate::GradleAdapter;
use crate::discover_gradle_projects;
use crate::load_configured_gradle_project;
use crate::parse_gradle_properties;

#[test]
fn versioned_properties_validate_the_selected_custom_keys() {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/native-audit-errors/gradle.properties");
	GradleAdapter
		.validate_versioned_file(
			&path,
			"gradle.properties",
			Some(&["CUSTOM_VERSION".to_string()]),
		)
		.unwrap_or_else(|error| panic!("custom version property: {error}"));
	let error = GradleAdapter
		.validate_versioned_file(&path, "gradle.properties", Some(&["MISSING".to_string()]))
		.expect_err("missing property");
	assert!(matches!(error, MonochangeError::Config(_)));
}

#[test]
fn discover_gradle_projects_reports_warnings_for_unreadable_properties() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("missing"), root.join("gradle.properties"))
		.unwrap_or_else(|error| panic!("symlink gradle.properties: {error}"));
	#[cfg(not(unix))]
	fs::write(
		root.join("gradle.properties"),
		include_bytes!("../../../../fixtures/tests/native-adapters/invalid-utf8"),
	)
	.unwrap_or_else(|error| panic!("write invalid gradle.properties: {error}"));

	let discovery =
		discover_gradle_projects(root).unwrap_or_else(|error| panic!("gradle discovery: {error}"));

	assert!(discovery.packages.is_empty());
	assert_eq!(discovery.warnings.len(), 1);
}

#[test]
fn load_configured_gradle_project_reads_the_manifest() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let android = root.join("android");
	fs::create_dir_all(&android).unwrap_or_else(|error| panic!("mkdir android: {error}"));
	fs::write(
		android.join("gradle.properties"),
		include_str!("../../../../fixtures/tests/native-adapters/gradle/load_configured_gradle_project_reads_the_manifest-13.txt"),
	)
	.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));

	let record = load_configured_gradle_project(root, &android)
		.unwrap_or_else(|error| panic!("load configured gradle: {error}"))
		.expect("gradle record");

	assert_eq!(record.name, "dev.canvist:canvist-android");
	assert_eq!(record.current_version, Some(Version::new(0, 2, 0)));
	assert_eq!(
		record.metadata.get("group_id").map(String::as_str),
		Some("dev.canvist")
	);
	assert_eq!(
		record.metadata.get("artifact_id").map(String::as_str),
		Some("canvist-android")
	);

	let missing = load_configured_gradle_project(root, &root.join("empty"))
		.unwrap_or_else(|error| panic!("load configured gradle: {error}"));
	assert!(missing.is_none());
}

#[test]
fn parse_gradle_properties_falls_back_to_settings_and_directory_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let android = root.join("android");
	fs::create_dir_all(&android).unwrap_or_else(|error| panic!("mkdir android: {error}"));
	fs::write(android.join("gradle.properties"), include_str!("../../../../fixtures/tests/native-adapters/gradle/parse_gradle_properties_falls_back_to_settings_and_directory_names-14.txt"))
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));
	fs::write(
		android.join("settings.gradle.kts"),
		include_str!("../../../../fixtures/tests/native-adapters/gradle/parse_gradle_properties_falls_back_to_settings_and_directory_names-15.txt"),
	)
	.unwrap_or_else(|error| panic!("write settings.gradle.kts: {error}"));

	let record = parse_gradle_properties(&android.join("gradle.properties"), root)
		.unwrap_or_else(|error| panic!("parse gradle.properties: {error}"))
		.expect("gradle record");

	assert_eq!(record.name, "canvist-android");
	assert!(!record.metadata.contains_key("group_id"));

	let nameless = root.join("nameless");
	fs::create_dir_all(&nameless).unwrap_or_else(|error| panic!("mkdir nameless: {error}"));
	fs::write(nameless.join("gradle.properties"), include_str!("../../../../fixtures/tests/native-adapters/gradle/parse_gradle_properties_falls_back_to_settings_and_directory_names-16.txt"))
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));

	let record = parse_gradle_properties(&nameless.join("gradle.properties"), root)
		.unwrap_or_else(|error| panic!("parse gradle.properties: {error}"))
		.expect("gradle record");

	assert_eq!(record.name, "nameless");
	assert_eq!(record.current_version, Some(Version::new(1, 0, 0)));
}
