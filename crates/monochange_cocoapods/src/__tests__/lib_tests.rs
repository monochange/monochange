use std::fs;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use monochange_core::PublishState;
use semver::Version;
use tempfile::tempdir;

use crate::CocoaPodsAdapter;
use crate::CocoaPodsVersionedFileKind;
use crate::adapter;
use crate::discover_pods;
use crate::load_configured_pod;
use crate::parse_cocoapods_version;
use crate::parse_podspec;
use crate::parse_podspec_name;
use crate::parse_podspec_version;
use crate::update_podspec_version;

const CANVIST_PODSPEC: &str =
	include_str!("../../../../fixtures/tests/native-adapters/cocoapods/canvist_podspec.txt");

// -- adapter --
#[test]
fn adapter_reports_cocoapods_ecosystem() {
	let cocoapods_adapter = CocoaPodsAdapter;
	assert_eq!(cocoapods_adapter.ecosystem(), Ecosystem::CocoaPods);
	assert_eq!(adapter().ecosystem(), Ecosystem::CocoaPods);
}

#[test]
fn adapter_reports_versioned_file_kinds() {
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("Canvist.podspec")),
		Some(CocoaPodsVersionedFileKind::Podspec)
	);
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("Canvist.podspec.json")),
		Some(CocoaPodsVersionedFileKind::PodspecJson)
	);
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("Canvist.json")),
		None
	);
	assert!(CocoaPodsAdapter.supported_versioned_file_kind(Path::new("Canvist.podspec")));
}

// -- discovery --
#[test]
fn discover_pods_finds_podspecs_with_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join("Canvist.podspec"), CANVIST_PODSPEC)
		.unwrap_or_else(|error| panic!("write podspec: {error}"));
	fs::write(root.join("Empty.podspec"), include_str!("../../../../fixtures/tests/native-adapters/cocoapods/discover_pods_finds_podspecs_with_names-6.txt"))
		.unwrap_or_else(|error| panic!("write empty podspec: {error}"));

	let discovery = discover_pods(root).unwrap_or_else(|error| panic!("pod discovery: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let package = discovery.packages.first().expect("pod");
	assert_eq!(package.name, "Canvist");
	assert_eq!(package.ecosystem, Ecosystem::CocoaPods);
	assert_eq!(package.current_version, Some(Version::new(0, 2, 0)));
	assert_eq!(package.publish_state, PublishState::Public);
}

#[test]
fn discover_pods_skips_pods_directories_and_reports_unreadable_podspecs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("Pods/Canvist"))
		.unwrap_or_else(|error| panic!("mkdir Pods: {error}"));
	fs::write(root.join("Pods/Canvist.podspec"), include_str!("../../../../fixtures/tests/native-adapters/cocoapods/discover_pods_skips_pods_directories_and_reports_unreadable_podspecs-7.txt"))
		.unwrap_or_else(|error| panic!("write nested podspec: {error}"));
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("missing"), root.join("Broken.podspec"))
		.unwrap_or_else(|error| panic!("symlink podspec: {error}"));
	#[cfg(not(unix))]
	fs::write(
		root.join("Broken.podspec"),
		include_bytes!("../../../../fixtures/tests/native-adapters/invalid-utf8"),
	)
	.unwrap_or_else(|error| panic!("write invalid podspec: {error}"));

	let discovery = discover_pods(root).unwrap_or_else(|error| panic!("pod discovery: {error}"));

	assert!(discovery.packages.is_empty());
	assert_eq!(discovery.warnings.len(), 1);
}

#[test]
fn load_configured_pod_reads_the_directory_podspec() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let pod_dir = root.join("apple");
	fs::create_dir_all(&pod_dir).unwrap_or_else(|error| panic!("mkdir apple: {error}"));
	fs::write(pod_dir.join("Canvist.podspec"), CANVIST_PODSPEC)
		.unwrap_or_else(|error| panic!("write podspec: {error}"));

	let record = load_configured_pod(root, &pod_dir)
		.unwrap_or_else(|error| panic!("load configured pod: {error}"));
	assert_eq!(record.expect("pod record").name, "Canvist");

	let missing = load_configured_pod(root, &root.join("empty"))
		.unwrap_or_else(|error| panic!("load configured pod: {error}"));
	assert!(missing.is_none());
}

#[test]
fn parse_podspec_resolves_derived_versions_to_none() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let podspec = root.join("Derived.podspec");
	fs::write(
		&podspec,
		include_str!("../../../../fixtures/tests/native-adapters/cocoapods/parse_podspec_resolves_derived_versions_to_none-8.txt"),
	)
	.unwrap_or_else(|error| panic!("write podspec: {error}"));

	let record = parse_podspec(&podspec, root)
		.unwrap_or_else(|error| panic!("parse podspec: {error}"))
		.expect("pod record");

	assert_eq!(record.name, "Derived");
	assert_eq!(record.current_version, None);
}

// -- parsing --
#[test]
fn parse_podspec_name_and_version_handle_quotes_and_receivers() {
	assert_eq!(
		parse_podspec_name("  s.name = 'Canvist'\n"),
		Some("Canvist".to_string())
	);
	assert_eq!(
		parse_podspec_name("  spec.name = \"Other\"\n"),
		Some("Other".to_string())
	);
	assert_eq!(
		parse_podspec_name("  pod.name = \"Third\"\n"),
		Some("Third".to_string())
	);
	assert_eq!(parse_podspec_name("summary = 'not a name'\n"), None);
	assert_eq!(parse_podspec_name("no declarations"), None);

	assert_eq!(
		parse_podspec_version("  s.version = '1.2.3'\n"),
		Some(Version::new(1, 2, 3))
	);
	assert_eq!(parse_podspec_version("  s.version = \"1.0\"\n"), None);
	assert_eq!(
		parse_podspec_version("  s.version = File.read('x')\n"),
		None
	);

	assert_eq!(
		parse_cocoapods_version("2.0.0-rc.1").map(|v| v.to_string()),
		Some("2.0.0-rc.1".to_string())
	);
	assert_eq!(parse_cocoapods_version("1"), None);
}

#[test]
fn update_podspec_version_preserves_quote_style() {
	let single = "  s.version = '1.2.3'\n  s.name = 'Canvist'\n";
	assert_eq!(
		update_podspec_version(single, "1.4.0"),
		"  s.version = '1.4.0'\n  s.name = 'Canvist'\n"
	);

	let double = "  s.version = \"0.1.0\"\n";
	assert_eq!(
		update_podspec_version(double, "0.2.0"),
		"  s.version = \"0.2.0\"\n"
	);

	let unchanged = "  s.name = 'Canvist'\n";
	assert_eq!(update_podspec_version(unchanged, "1.0.0"), unchanged);
}

// -- validation --
#[test]
fn validate_versioned_file_requires_a_name_declaration() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("Canvist.podspec");
	fs::write(&path, CANVIST_PODSPEC).unwrap_or_else(|error| panic!("write podspec: {error}"));

	CocoaPodsAdapter
		.validate_versioned_file(&path, "Canvist.podspec", None)
		.unwrap_or_else(|error| panic!("validate podspec: {error}"));

	let nameless = tempdir.path().join("Nameless.podspec");
	fs::write(&nameless, include_str!("../../../../fixtures/tests/native-adapters/cocoapods/validate_versioned_file_requires_a_name_declaration-9.txt"))
		.unwrap_or_else(|error| panic!("write nameless podspec: {error}"));

	let error = CocoaPodsAdapter
		.validate_versioned_file(&nameless, "Nameless.podspec", None)
		.expect_err("nameless podspec must fail validation");

	assert!(error.to_string().contains("no `name` declaration"));
	assert!(matches!(error, MonochangeError::Config(_)));
}

// -- defaults --
#[test]
fn dependency_defaults_are_empty() {
	assert_eq!(crate::default_dependency_version_prefix(), "");
	assert!(crate::default_dependency_fields().is_empty());
}

#[test]
fn json_podspecs_expose_name_and_version() {
	let contents =
		include_str!("../../../../fixtures/tests/native-audit/json/Canvist.podspec.json");
	assert_eq!(parse_podspec_name(contents).as_deref(), Some("CanvistJson"));
	assert_eq!(parse_podspec_version(contents), Some(Version::new(1, 2, 3)));
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/native-audit/json/Canvist.podspec.json");
	let package = parse_podspec(&path, path.parent().expect("parent"))
		.unwrap_or_else(|error| panic!("parse json podspec: {error}"))
		.expect("pod record");
	assert_eq!(package.name, "CanvistJson");
	CocoaPodsAdapter
		.validate_versioned_file(
			&path,
			"Canvist.podspec.json",
			Some(&["version".to_string()]),
		)
		.unwrap_or_else(|error| panic!("validate json versioned file: {error}"));
}

#[test]
fn typed_podspec_updates_reject_computed_versions() {
	let path = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/native-audit-errors/Derived.podspec");
	let error = CocoaPodsAdapter
		.validate_versioned_file(&path, "Derived.podspec", Some(&["version".to_string()]))
		.expect_err("computed version cannot be rewritten as a literal");
	assert!(matches!(error, MonochangeError::Config(_)));
}

#[test]
fn podspec_updates_skip_comments_and_unrelated_attributes() {
	let contents = "# s.version = '0.0.0'\n  s.swift_version = '5.0'\n  s.version = '1.2.3'\n";
	assert_eq!(
		update_podspec_version(contents, "1.4.0"),
		"# s.version = '0.0.0'\n  s.swift_version = '5.0'\n  s.version = '1.4.0'\n"
	);
}

#[test]
fn validate_versioned_file_reports_unreadable_podspecs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	// A directory cannot be read as podspec text.
	let error = CocoaPodsAdapter
		.validate_versioned_file(tempdir.path(), "Canvist.podspec", None)
		.expect_err("an unreadable podspec must fail validation");

	assert!(matches!(error, MonochangeError::Io(_)));
	assert!(error.to_string().contains("failed to read"));
}
