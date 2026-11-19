use std::fs;

use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use tempfile::tempdir;

use crate::CocoaPodsAdapter;
use crate::discover_pods;
use crate::load_configured_pod;

const CANVIST_PODSPEC: &str =
	include_str!("../../../../fixtures/tests/native-adapters/cocoapods/canvist_podspec.txt");

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
