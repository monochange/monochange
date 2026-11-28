use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishState;
use monochange_core::PublishTimeoutSettings;
use monochange_core::RegistryKind;
use monochange_core::TrustedPublishingSettings;
use monochange_publish::PublishRequest;
use semver::Version;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;
use tempfile::tempdir;

fn yaml_bool(mapping: &Mapping, key: &str) -> Option<bool> {
	mapping
		.get(Value::String(key.to_string()))
		.and_then(Value::as_bool)
}

use crate::default_lockfile_commands;
use crate::discover_workspace_packages;
use crate::has_workspace_section;
use crate::parse_manifest;
use crate::parse_yaml_manifest;
use crate::update_dependency_fields;
use crate::update_manifest_text;
use crate::write_dart_placeholder_manifest;
use crate::yaml_array_strings;
use crate::yaml_mapping;
use crate::yaml_string;

fn sample_publish_request(root: &Path) -> PublishRequest {
	PublishRequest {
		package_id: "dart-package".to_string(),
		package_name: "dart_package".to_string(),
		ecosystem: Ecosystem::Dart,
		manifest_path: root.join("source-pubspec.yaml"),
		package_root: root.to_path_buf(),
		registry: RegistryKind::PubDev,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		flow: monochange_core::PublishFlow::Direct,
		version: "0.0.0".to_string(),
		placeholder: true,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: String::new(),
		release_tag: None,
		run_scope: monochange_publish::PublishRunScope::Publish,
	}
}

#[test]
fn dart_placeholder_manifest_uses_source_environment() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let request = sample_publish_request(tempdir.path());
	fs::write(
		&request.manifest_path,
		"name: dart_package\nenvironment:\n  sdk: ^3.11.0\n  flutter: '>=3.30.0'\n",
	)
	.unwrap_or_else(|error| panic!("write source pubspec: {error}"));

	write_dart_placeholder_manifest(tempdir.path(), &request, None)
		.unwrap_or_else(|error| panic!("write placeholder manifest: {error}"));

	let manifest = fs::read_to_string(tempdir.path().join("pubspec.yaml"))
		.unwrap_or_else(|error| panic!("read pubspec: {error}"));

	assert!(manifest.contains("environment:\n  sdk: ^3.11.0\n  flutter: '>=3.30.0'\n"));
}

#[test]
fn default_lockfile_commands_choose_dart_or_flutter_pub_get() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/manifest-lockfile-workspace");
	let dart_package = PackageRecord::new(
		Ecosystem::Dart,
		"nested_dart_app",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&dart_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "dart pub get".to_string(),
			cwd: monochange_core::normalize_path(&fixture_root.join("packages/app")),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let mut flutter_package = PackageRecord::new(
		Ecosystem::Dart,
		"nested_flutter_app",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	flutter_package
		.metadata
		.insert("is_flutter".to_string(), "true".to_string());
	assert_eq!(
		default_lockfile_commands(&flutter_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "flutter pub get".to_string(),
			cwd: monochange_core::normalize_path(&fixture_root.join("packages/app")),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let cargo_package = PackageRecord::new(
		Ecosystem::Cargo,
		"not-dart",
		fixture_root.join("packages/app/pubspec.yaml"),
		fixture_root,
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert!(default_lockfile_commands(&cargo_package).is_empty());
}

#[test]
fn update_dependency_fields_only_changes_declared_dependencies() {
	let mut manifest: Mapping = serde_yaml_ng::from_str(
		r"
dependencies:
  core: ^1.0.0
dev_dependencies:
  test: ^1.0.0
",
	)
	.unwrap_or_else(|error| panic!("pubspec yaml: {error}"));
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_dependency_fields(
		&mut manifest,
		&["dependencies", "dev_dependencies"],
		&versions,
	);

	let rendered = serde_yaml_ng::to_string(&Value::Mapping(manifest))
		.unwrap_or_else(|error| panic!("render manifest: {error}"));
	assert!(rendered.contains("core: 2.0.0"));
	assert!(rendered.contains("test: ^1.0.0"));
}

#[test]
fn update_manifest_text_preserves_pubspec_formatting() {
	let manifest = r"name: sample_app
version: '1.0.0' # keep quote

dependencies:
  shared:
    path: ../shared
    version: ^1.0.0
  http: ^1.0.0

dev_dependencies:
  test: ^1.0.0
";
	let updated = update_manifest_text(
		manifest,
		Some("2.0.0"),
		&["dependencies", "dev_dependencies"],
		&BTreeMap::from([
			("shared".to_string(), "^2.0.0".to_string()),
			("test".to_string(), "^2.0.0".to_string()),
		]),
	)
	.unwrap_or_else(|error| panic!("update pubspec text: {error}"));
	assert!(updated.contains("version: '2.0.0' # keep quote"));
	assert!(updated.contains("path: ../shared"));
	assert!(updated.contains("version: ^2.0.0"));
	assert!(updated.contains("test: ^2.0.0"));
	assert!(updated.contains("http: ^1.0.0"));
}

#[test]
fn workspace_and_manifest_helpers_cover_yaml_and_error_paths() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/workspace-pattern-warnings");
	let workspace_manifest = fixture_root.join("pubspec.yaml");
	assert!(has_workspace_section(&workspace_manifest).unwrap());
	let parsed = parse_yaml_manifest(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace yaml: {error}"));
	assert_eq!(
		yaml_array_strings(&parsed, "workspace"),
		vec!["packages/*".to_string(), "missing/*".to_string()]
	);
	assert_eq!(yaml_string(&parsed, "name"), None);
	assert_eq!(yaml_bool(&parsed, "publish_to"), None);
	assert_eq!(yaml_mapping(&parsed, "dependencies"), None);

	let app_manifest = fixture_root.join("packages/app/pubspec.yaml");
	let app = parse_manifest(&app_manifest, &fixture_root)
		.unwrap_or_else(|error| panic!("parse app manifest: {error}"))
		.unwrap_or_else(|| panic!("expected app package"));
	assert_eq!(app.ecosystem, Ecosystem::Dart);
	assert_eq!(app.publish_state, PublishState::Public);
	assert_eq!(
		app.current_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("1.2.3")
	);
	assert!(app.declared_dependencies.iter().any(|dependency| {
		dependency.name == "shared"
			&& dependency.version_constraint.as_deref() == Some("^1.0.0")
			&& dependency.source_field.as_deref() == Some("dependencies")
	}));

	let private_manifest = fixture_root.join("packages/private/pubspec.yaml");
	let private = parse_manifest(&private_manifest, &fixture_root)
		.unwrap_or_else(|error| panic!("parse private manifest: {error}"))
		.unwrap_or_else(|| panic!("expected private package"));
	assert_eq!(private.ecosystem, Ecosystem::Dart);
	assert_eq!(private.publish_state, PublishState::Private);
	assert_eq!(private.current_version, None);

	let workspace_root_manifest =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dart/monorepo/pubspec.yaml");
	let workspace_root = parse_manifest(
		&workspace_root_manifest,
		workspace_root_manifest
			.parent()
			.expect("workspace root parent"),
	)
	.unwrap_or_else(|error| panic!("parse workspace root manifest: {error}"))
	.unwrap_or_else(|| panic!("expected workspace root package"));
	assert_eq!(workspace_root.publish_state, PublishState::Private);

	let discovery = discover_workspace_packages(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace discovery: {error}"));
	assert_eq!(discovery.0.len(), 2);
	assert!(discovery.1.iter().any(|warning| {
		warning.contains("missing/*") && warning.contains("matched no packages")
	}));

	let nameless_manifest: Mapping = serde_yaml_ng::from_str(
		r"
dependencies:
  core: ^1.0.0
",
	)
	.unwrap_or_else(|error| panic!("yaml: {error}"));
	assert_eq!(yaml_string(&nameless_manifest, "name"), None);
	assert!(yaml_mapping(&nameless_manifest, "dependencies").is_some());
	assert_eq!(yaml_bool(&nameless_manifest, "publish_to"), None);

	let invalid_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/invalid-workspace/invalid-workspace.yaml");
	let invalid_workspace_error = has_workspace_section(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected invalid workspace error"));
	assert!(
		invalid_workspace_error
			.to_string()
			.contains("failed to parse")
	);

	let invalid_package = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/dart/invalid-package/invalid-package.yaml");
	let invalid_package_error = parse_manifest(&invalid_package, Path::new("."))
		.err()
		.unwrap_or_else(|| panic!("expected invalid package error"));
	assert!(
		invalid_package_error
			.to_string()
			.contains("failed to parse")
	);
}

#[test]
fn validate_versioned_file_accepts_dependency_section_field() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("pubspec.yaml");
	fs::write(
		&path,
		"name: app\nversion: 1.0.0\ndependencies:\n  core: ^1.0.0\n",
	)
	.unwrap_or_else(|error| panic!("write: {error}"));
	let custom_fields = vec!["version".to_string(), "dependencies".to_string()];
	assert!(super::validate_versioned_file(&path, "pubspec.yaml", Some(&custom_fields)).is_ok());
}
