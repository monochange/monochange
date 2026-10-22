use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::MonochangeError;
use monochange_core::PublishState;
use monochange_publish::PublishRequest;
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
use crate::write_cocoapods_placeholder_manifest;

fn publish_request(name: &str, version: &str) -> PublishRequest {
	PublishRequest {
		package_id: format!("cocoapods:{name}"),
		package_name: name.to_string(),
		ecosystem: Ecosystem::CocoaPods,
		manifest_path: PathBuf::from(format!("{name}.podspec")),
		package_root: PathBuf::from("."),
		registry: monochange_core::RegistryKind::CocoaPodsTrunk,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: monochange_core::PublishMode::Builtin,
		flow: monochange_core::PublishFlow::Direct,
		version: version.to_string(),
		placeholder: false,
		trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
		attestations: monochange_core::PublishAttestationSettings::default(),
		timeout: monochange_core::PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: String::new(),
	}
}

const CANVIST_PODSPEC: &str = r#"
Pod::Spec.new do |s|
  s.name = "Canvist"
  s.version = "0.2.0"
  s.summary = "canvist editor for iOS"
end
"#;

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
	fs::write(root.join("Empty.podspec"), "# no declarations\n")
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
	fs::write(root.join("Pods/Canvist.podspec"), "s.name = 'Nested'\n")
		.unwrap_or_else(|error| panic!("write nested podspec: {error}"));
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("missing"), root.join("Broken.podspec"))
		.unwrap_or_else(|error| panic!("symlink podspec: {error}"));
	#[cfg(not(unix))]
	fs::write(root.join("Broken.podspec"), [0xff, 0xfe])
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
		"version = File.read('Cargo.toml')\nPod::Spec.new do |s|\n  s.name = 'Derived'\nend\n",
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

// -- placeholder manifest --
#[test]
fn write_cocoapods_placeholder_manifest_writes_a_complete_podspec() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let dir = tempdir.path();
	write_cocoapods_placeholder_manifest(dir, &publish_request("Canvist", "0.0.0"))
		.unwrap_or_else(|error| panic!("write placeholder podspec: {error}"));

	let contents = fs::read_to_string(dir.join("Canvist.podspec"))
		.unwrap_or_else(|error| panic!("read placeholder podspec: {error}"));

	assert!(contents.contains("s.name = 'Canvist'"));
	assert!(contents.contains("s.version = '0.0.0'"));
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
	fs::write(&nameless, "  s.version = '1.0.0'\n")
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
fn validate_versioned_file_reports_unreadable_podspecs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	// A directory cannot be read as podspec text.
	let error = CocoaPodsAdapter
		.validate_versioned_file(tempdir.path(), "Canvist.podspec", None)
		.expect_err("an unreadable podspec must fail validation");

	assert!(matches!(error, MonochangeError::Io(_)));
	assert!(error.to_string().contains("failed to read"));
}
