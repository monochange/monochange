use std::fs;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use monochange_core::PublishState;
use semver::Version;
use tempfile::tempdir;

use crate::GradleAdapter;
use crate::GradleVersionedFileKind;
use crate::adapter;
use crate::discover_gradle_projects;
use crate::load_configured_gradle_project;
use crate::maven_central_metadata_path;
use crate::parse_gradle_properties;
use crate::parse_properties;
use crate::parse_root_project_name;
use crate::project_name_from_settings;
use crate::update_gradle_properties_text;

const CANVIST_PROPERTIES: &str =
	"org.gradle.jvmargs=-Xmx4g\nandroid.useAndroidX=true\nVERSION_NAME=0.1.0\n";

// -- adapter --
#[test]
fn adapter_reports_gradle_ecosystem() {
	let gradle_adapter = GradleAdapter;
	assert_eq!(gradle_adapter.ecosystem(), Ecosystem::Gradle);
	assert_eq!(adapter().ecosystem(), Ecosystem::Gradle);
}

#[test]
fn adapter_reports_versioned_file_kinds() {
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("gradle.properties")),
		Some(GradleVersionedFileKind::GradleProperties)
	);
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("local.properties")),
		None
	);
	assert!(GradleAdapter.supported_versioned_file_kind(Path::new("gradle.properties")));
}

// -- discovery --
#[test]
fn discover_gradle_projects_finds_versioned_properties() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let android = root.join("android");
	fs::create_dir_all(&android).unwrap_or_else(|error| panic!("mkdir android: {error}"));
	fs::write(android.join("gradle.properties"), CANVIST_PROPERTIES)
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));
	fs::write(root.join("local.properties"), "VERSION_NAME=9.9.9\n")
		.unwrap_or_else(|error| panic!("write local.properties: {error}"));
	fs::create_dir_all(root.join("build/outputs"))
		.unwrap_or_else(|error| panic!("mkdir build: {error}"));
	fs::write(root.join("build/gradle.properties"), "VERSION_NAME=8.8.8\n")
		.unwrap_or_else(|error| panic!("write build gradle.properties: {error}"));

	let discovery =
		discover_gradle_projects(root).unwrap_or_else(|error| panic!("gradle discovery: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let package = discovery.packages.first().expect("gradle project");
	assert_eq!(package.name, "android");
	assert_eq!(package.ecosystem, Ecosystem::Gradle);
	assert_eq!(package.current_version, Some(Version::new(0, 1, 0)));
	assert_eq!(package.publish_state, PublishState::Public);
}

#[test]
fn discover_gradle_projects_skips_properties_without_a_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join("gradle.properties"), "org.gradle.caching=true\n")
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));

	let discovery =
		discover_gradle_projects(root).unwrap_or_else(|error| panic!("gradle discovery: {error}"));

	assert!(discovery.packages.is_empty());
}

#[test]
fn discover_gradle_projects_reports_warnings_for_unreadable_properties() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("missing"), root.join("gradle.properties"))
		.unwrap_or_else(|error| panic!("symlink gradle.properties: {error}"));
	#[cfg(not(unix))]
	fs::write(root.join("gradle.properties"), [0xff, 0xfe])
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
		"GROUP=dev.canvist\nPOM_ARTIFACT_ID=canvist-android\nVERSION_NAME=0.2.0\n",
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
	fs::write(android.join("gradle.properties"), "VERSION_NAME=1.0.0\n")
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));
	fs::write(
		android.join("settings.gradle.kts"),
		"rootProject.name = \"canvist-android\"\n",
	)
	.unwrap_or_else(|error| panic!("write settings.gradle.kts: {error}"));

	let record = parse_gradle_properties(&android.join("gradle.properties"), root)
		.unwrap_or_else(|error| panic!("parse gradle.properties: {error}"))
		.expect("gradle record");

	assert_eq!(record.name, "canvist-android");
	assert!(!record.metadata.contains_key("group_id"));

	let nameless = root.join("nameless");
	fs::create_dir_all(&nameless).unwrap_or_else(|error| panic!("mkdir nameless: {error}"));
	fs::write(nameless.join("gradle.properties"), "VERSION_NAME=1.0.0\n")
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));

	let record = parse_gradle_properties(&nameless.join("gradle.properties"), root)
		.unwrap_or_else(|error| panic!("parse gradle.properties: {error}"))
		.expect("gradle record");

	assert_eq!(record.name, "nameless");
	assert_eq!(record.current_version, Some(Version::new(1, 0, 0)));
}

#[test]
fn parse_gradle_properties_reports_invalid_versions_as_none() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join("gradle.properties"), "VERSION_NAME=1.0\n")
		.unwrap_or_else(|error| panic!("write gradle.properties: {error}"));

	let record = parse_gradle_properties(&root.join("gradle.properties"), root)
		.unwrap_or_else(|error| panic!("parse gradle.properties: {error}"))
		.expect("gradle record");

	assert_eq!(record.current_version, None);
}

// -- properties parsing --
#[test]
fn parse_properties_handles_separators_comments_and_quotes() {
	let properties = parse_properties(
		"# comment\n! bang comment\n\norg.gradle.jvmargs=-Xmx4g\nhost:example.com\nspaced value\nquoted=\"wrapped\"\n",
	);

	assert_eq!(
		properties.get("org.gradle.jvmargs").map(String::as_str),
		Some("-Xmx4g")
	);
	assert_eq!(
		properties.get("host").map(String::as_str),
		Some("example.com")
	);
	assert_eq!(properties.get("spaced").map(String::as_str), Some("value"));
	assert_eq!(
		properties.get("quoted").map(String::as_str),
		Some("wrapped")
	);
	assert!(!properties.contains_key("#"));
}

#[test]
fn parse_root_project_name_reads_both_settings_dialects() {
	assert_eq!(
		parse_root_project_name("rootProject.name = \"canvist-android\"\n"),
		Some("canvist-android".to_string())
	);
	assert_eq!(
		parse_root_project_name("rootProject.name = 'legacy'\n"),
		Some("legacy".to_string())
	);
	assert_eq!(parse_root_project_name("include(\":app\")\n"), None);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let dir = tempdir.path();
	assert_eq!(project_name_from_settings(dir), None);
	fs::write(dir.join("settings.gradle"), "rootProject.name = \"kts\"\n")
		.unwrap_or_else(|error| panic!("write settings.gradle: {error}"));
	assert_eq!(project_name_from_settings(dir), Some("kts".to_string()));
}

// -- versioned file updates --
#[test]
fn update_gradle_properties_text_rewrites_listed_keys_only() {
	let contents = "org.gradle.caching=true\nVERSION_NAME=0.1.0\nVERSION_CODE:12\n";
	let updated = update_gradle_properties_text(contents, &["VERSION_NAME"], "0.2.0");

	assert_eq!(
		updated,
		"org.gradle.caching=true\nVERSION_NAME=0.2.0\nVERSION_CODE:12\n"
	);
	assert_eq!(
		update_gradle_properties_text(contents, &[], "0.2.0"),
		contents
	);

	let colon = update_gradle_properties_text("VERSION_CODE:12\n", &["VERSION_CODE"], "13");
	assert_eq!(colon, "VERSION_CODE:13\n");

	let indented =
		update_gradle_properties_text("  VERSION_NAME=0.1.0", &["VERSION_NAME"], "0.2.0");
	assert_eq!(indented, "  VERSION_NAME=0.2.0");
}

// -- maven paths --
#[test]
fn maven_central_metadata_path_renders_group_segments() {
	assert_eq!(
		maven_central_metadata_path("dev.canvist", "canvist-android"),
		"dev/canvist/canvist-android"
	);
	assert_eq!(
		maven_central_metadata_path("io.sentry", "sentry"),
		"io/sentry/sentry"
	);
}

// -- validation --
#[test]
fn validate_versioned_file_requires_version_name() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("gradle.properties");
	fs::write(&path, CANVIST_PROPERTIES)
		.unwrap_or_else(|error| panic!("write properties: {error}"));

	GradleAdapter
		.validate_versioned_file(&path, "gradle.properties", None)
		.unwrap_or_else(|error| panic!("validate properties: {error}"));

	let versionless = tempdir.path().join("other.properties");
	fs::write(&versionless, "org.gradle.caching=true\n")
		.unwrap_or_else(|error| panic!("write versionless properties: {error}"));

	let error = GradleAdapter
		.validate_versioned_file(&versionless, "other.properties", None)
		.expect_err("versionless properties must fail validation");

	assert!(error.to_string().contains("no `VERSION_NAME` property"));
	assert!(matches!(error, MonochangeError::Config(_)));
}

// -- defaults --
#[test]
fn dependency_defaults_are_empty() {
	assert_eq!(crate::default_dependency_version_prefix(), "");
	assert!(crate::default_dependency_fields().is_empty());
}

#[test]
fn validate_versioned_file_reports_unreadable_properties() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = GradleAdapter
		.validate_versioned_file(tempdir.path(), "gradle.properties", None)
		.expect_err("an unreadable properties file must fail validation");

	assert!(matches!(error, MonochangeError::Io(_)));
	assert!(error.to_string().contains("failed to read"));
}

#[test]
fn parse_properties_skips_lines_without_key_value_pairs() {
	let properties = parse_properties("bare\nother");

	assert!(properties.is_empty());
}

#[test]
fn project_name_from_settings_ignores_scripts_without_root_project_name() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let dir = tempdir.path();
	fs::write(dir.join("settings.gradle.kts"), "include(\":app\")\n")
		.unwrap_or_else(|error| panic!("write settings.gradle.kts: {error}"));

	assert_eq!(project_name_from_settings(dir), None);
}
