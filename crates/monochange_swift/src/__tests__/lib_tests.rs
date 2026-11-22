use std::fs;
use std::path::Path;

use monochange_core::EcosystemAdapter;
use tempfile::tempdir;

use crate::SwiftAdapter;
use crate::SwiftVersionedFileKind;
use crate::discover_swift_packages;
use crate::load_configured_swift_package;

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
