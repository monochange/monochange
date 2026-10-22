use std::fs;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PublishState;
use tempfile::tempdir;

use crate::SwiftAdapter;
use crate::SwiftVersionedFileKind;
use crate::adapter;
use crate::discover_swift_packages;
use crate::load_configured_swift_package;
use crate::parse_package_name;
use crate::parse_package_swift;

#[test]
fn package_name_comes_from_package_initializer() {
	let contents = include_str!("../../../../fixtures/tests/native-audit/swift/Package.swift");
	assert_eq!(parse_package_name(contents).as_deref(), Some("NativeSwift"));
}

// -- adapter --
#[test]
fn adapter_reports_swift_ecosystem() {
	let swift_adapter = SwiftAdapter;
	assert_eq!(swift_adapter.ecosystem(), Ecosystem::Swift);
	assert_eq!(adapter().ecosystem(), Ecosystem::Swift);
}

#[test]
fn adapter_validates_any_versioned_file_and_reports_kinds() {
	let swift_adapter = SwiftAdapter;
	assert!(swift_adapter.supported_versioned_file_kind(Path::new("Package.swift")));
	assert!(!swift_adapter.supported_versioned_file_kind(Path::new("Package.resolved")));

	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("Package.swift")),
		Some(SwiftVersionedFileKind::PackageSwift)
	);
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("Package.swift")),
		Some(SwiftVersionedFileKind::PackageSwift)
	);
	assert_eq!(
		crate::supported_versioned_file_kind(Path::new("notes.txt")),
		None
	);
}

#[test]
fn adapter_validate_versioned_file_accepts_any_file() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("Package.swift");
	fs::write(&path, include_str!("../../../../fixtures/tests/native-adapters/swift/adapter_validate_versioned_file_accepts_any_file-1.txt"))
		.unwrap_or_else(|error| panic!("write Package.swift: {error}"));

	SwiftAdapter
		.validate_versioned_file(&path, "Package.swift", None)
		.unwrap_or_else(|error| panic!("validate: {error}"));
}

// -- discovery --
#[test]
fn discover_swift_packages_finds_named_manifests() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("Package.swift"),
		include_str!("../../../../fixtures/tests/native-adapters/swift/discover_swift_packages_finds_named_manifests-2.txt"),
	)
	.unwrap_or_else(|error| panic!("write Package.swift: {error}"));

	let discovery =
		discover_swift_packages(root).unwrap_or_else(|error| panic!("swift discovery: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let package = discovery.packages.first().expect("swift package");
	assert_eq!(package.name, "Canvist");
	assert_eq!(package.ecosystem, Ecosystem::Swift);
	assert_eq!(package.current_version, None);
	assert_eq!(package.publish_state, PublishState::Public);
}

#[test]
fn discover_swift_packages_skips_build_directories_and_nameless_manifests() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join(".build/checkouts"))
		.unwrap_or_else(|error| panic!("mkdir .build: {error}"));
	fs::write(root.join(".build/Package.swift"), include_str!("../../../../fixtures/tests/native-adapters/swift/discover_swift_packages_skips_build_directories_and_nameless_manifests-3.txt"))
		.unwrap_or_else(|error| panic!("write nested Package.swift: {error}"));
	fs::write(root.join("Package.swift"), include_str!("../../../../fixtures/tests/native-adapters/swift/discover_swift_packages_skips_build_directories_and_nameless_manifests-4.txt"))
		.unwrap_or_else(|error| panic!("write Package.swift: {error}"));

	let discovery =
		discover_swift_packages(root).unwrap_or_else(|error| panic!("swift discovery: {error}"));

	assert!(discovery.packages.is_empty());
	assert!(discovery.warnings.is_empty());
}

#[test]
fn discover_swift_packages_reports_warnings_for_unreadable_manifests() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("missing-package"), root.join("Package.swift"))
		.unwrap_or_else(|error| panic!("symlink Package.swift: {error}"));
	#[cfg(not(unix))]
	fs::write(
		root.join("Package.swift"),
		include_bytes!("../../../../fixtures/tests/native-adapters/invalid-utf8"),
	)
	.unwrap_or_else(|error| panic!("write invalid Package.swift: {error}"));

	let discovery =
		discover_swift_packages(root).unwrap_or_else(|error| panic!("swift discovery: {error}"));

	assert!(discovery.packages.is_empty());
	assert_eq!(discovery.warnings.len(), 1);
}

#[test]
fn load_configured_swift_package_reads_the_manifest() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let package_dir = root.join("apple");
	fs::create_dir_all(&package_dir).unwrap_or_else(|error| panic!("mkdir apple: {error}"));
	fs::write(
		package_dir.join("Package.swift"),
		include_str!("../../../../fixtures/tests/native-adapters/swift/load_configured_swift_package_reads_the_manifest-5.txt"),
	)
	.unwrap_or_else(|error| panic!("write Package.swift: {error}"));

	let record = load_configured_swift_package(root, &package_dir)
		.unwrap_or_else(|error| panic!("load configured: {error}"));

	assert_eq!(record.expect("swift record").name, "Canvist");

	let missing = load_configured_swift_package(root, &root.join("empty"))
		.unwrap_or_else(|error| panic!("load configured: {error}"));
	assert!(missing.is_none());
}

// -- parsing --
#[test]
fn parse_package_name_reads_the_first_uncommented_label() {
	let contents = r#"
// name: "Commented"
let package = Package(
    name: "Canvist",
    platforms: [.macOS(.v13)]
)
"#;
	assert_eq!(parse_package_name(contents), Some("Canvist".to_string()));
}

#[test]
fn parse_package_name_reads_multiline_and_inline_declarations() {
	assert_eq!(parse_package_name("name: \"Solo\"\n"), None);
	assert_eq!(
		parse_package_name("let package = Package(name: \"Inline\", products: [])"),
		Some("Inline".to_string())
	);
}

#[test]
fn parse_package_name_skips_urls_and_reports_nothing_without_a_name() {
	let contents = r#"
.dependencies: [
    .package(url: "https://github.com/apple/swift-argument-parser", from: "1.0.0"),
]
"#;
	assert_eq!(parse_package_name(contents), None);
	assert_eq!(parse_package_name("// name: \"Only Commented\"\n"), None);
}

#[test]
fn parse_package_swift_resolves_versions_from_tags_not_the_manifest() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let manifest = root.join("Package.swift");
	fs::copy(
		Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../../fixtures/tests/native-release/swift/Package.swift"),
		&manifest,
	)
	.unwrap_or_else(|error| panic!("write Package.swift: {error}"));

	let record = parse_package_swift(&manifest, root)
		.unwrap_or_else(|error| panic!("parse Package.swift: {error}"))
		.expect("swift package");

	assert_eq!(record.name, "Canvist");
	assert_eq!(record.current_version, None);
	assert!(record.manifest_path.ends_with("Package.swift"));
}

// -- defaults --
#[test]
fn dependency_defaults_are_empty() {
	assert_eq!(crate::default_dependency_version_prefix(), "");
	assert!(crate::default_dependency_fields().is_empty());
}
