use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::ChangeSignal;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_semver::CompatibilityProvider;
use tempfile::tempdir;
use toml::Value;
use toml_edit::DocumentMut;
use toml_edit::Item;

use crate::RustSemverProvider;
use crate::default_lockfile_commands;
use crate::discover_cargo_packages;
use crate::discover_workspace_packages;
use crate::has_workspace_section;
use crate::parse_package_manifest;
use crate::workspace_package_version;

#[test]
fn rust_semver_provider_parses_compatibility_evidence() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace");
	let discovery = discover_cargo_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("cargo discovery: {error}"));
	let package = discovery
		.packages
		.iter()
		.find(|package| package.name == "cargo-core")
		.unwrap_or_else(|| panic!("expected cargo-core package"));
	let signal = ChangeSignal {
		package_id: package.id.clone(),
		requested_bump: None,
		explicit_version: None,
		change_origin: "direct-change".to_string(),
		evidence_refs: vec!["rust-semver:major:public API break detected".to_string()],
		notes: Some("breaking change".to_string()),
		details: None,
		change_type: None,
		caused_by: Vec::new(),
		source_path: PathBuf::from(".changeset/feature.md"),
	};
	let provider = RustSemverProvider;
	let assessment = provider
		.assess(package, &signal)
		.unwrap_or_else(|| panic!("expected semver assessment"));

	assert_eq!(provider.provider_id(), "rust-semver");
	assert_eq!(assessment.severity.to_string(), "major");
	assert_eq!(assessment.summary, "public API break detected");
}

#[test]
fn default_lockfile_commands_use_cargo_generate_lockfile_in_lockfile_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/manifest-lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"lockfile-core",
		fixture_root.join("crates/core/Cargo.toml"),
		fixture_root.clone(),
		None,
		PublishState::Public,
	);

	assert_eq!(
		default_lockfile_commands(&package),
		vec![monochange_core::LockfileCommandExecution {
			command: "cargo generate-lockfile".to_string(),
			cwd: monochange_core::normalize_path(&fixture_root.join("crates/core")),
			shell: monochange_core::ShellConfig::None,
		}]
	);
}

#[test]
fn cargo_versioned_file_helpers_cover_non_table_and_insertion_paths() {
	let mut dependency_document = "[dependencies]\nweird = true\n"
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("parse dependency document: {error}"));
	let dependency_table = dependency_document
		.get_mut("dependencies")
		.and_then(Item::as_table_like_mut)
		.unwrap_or_else(|| panic!("expected dependencies table"));
	let weird_entry = dependency_table
		.get_mut("weird")
		.unwrap_or_else(|| panic!("expected weird entry"));
	crate::update_dependency_entry(weird_entry, "2.0.0");
	assert_eq!(weird_entry.as_bool(), Some(true));

	let mut insert_document = "[dependencies]\n"
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("parse insert document: {error}"));
	let insert_table = insert_document
		.get_mut("dependencies")
		.and_then(Item::as_table_like_mut)
		.unwrap_or_else(|| panic!("expected insert dependencies table"));
	crate::set_table_value(insert_table, "core", "2.0.0");
	assert_eq!(
		insert_document
			.get("dependencies")
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("core"))
			.and_then(Item::as_str),
		Some("2.0.0")
	);

	let mut nested_document = r#"
[workspace]
version = "1.0.0"

[workspace.metadata]
value = true
"#
	.parse::<DocumentMut>()
	.unwrap_or_else(|error| panic!("parse nested document: {error}"));
	let workspace_table = nested_document
		.get_mut("workspace")
		.and_then(Item::as_table_like_mut)
		.unwrap_or_else(|| panic!("expected workspace table"));
	crate::set_table_value_by_path(workspace_table, &[], "2.0.0");
	crate::set_table_value_by_path(workspace_table, &["missing", "version"], "2.0.0");
	crate::set_table_value_by_path(workspace_table, &["version", "nested"], "2.0.0");
	crate::set_table_value_by_path(workspace_table, &["metadata", "version"], "2.0.0");
	assert_eq!(
		nested_document
			.get("workspace")
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("version"))
			.and_then(Item::as_str),
		Some("1.0.0")
	);
	assert_eq!(
		nested_document
			.get("workspace")
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("metadata"))
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("value"))
			.and_then(Item::as_bool),
		Some(true)
	);

	let mut empty_item = Item::None;
	crate::set_item_string(&mut empty_item, "3.0.0");
	assert_eq!(empty_item.as_str(), Some("3.0.0"));
}

#[test]
fn cargo_manifest_field_helpers_cover_direct_path_updates() {
	let mut document = r#"
[package]
name = "app"
version = "1.0.0"

[dependencies]
core = { version = "1.0.0" }

[workspace.dependencies]
core = { version = "1.0.0" }
"#
	.parse::<DocumentMut>()
	.unwrap_or_else(|error| panic!("parse field helper document: {error}"));
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	crate::update_manifest_field(
		&mut document,
		"package.version",
		Some("2.0.0"),
		None,
		&versions,
	);
	crate::update_manifest_field(
		&mut document,
		"workspace.dependencies",
		None,
		None,
		&versions,
	);
	crate::update_manifest_field(&mut document, "dependencies.core", None, None, &versions);
	crate::update_manifest_field(
		&mut document,
		"dependencies.core.version",
		None,
		None,
		&versions,
	);
	crate::update_manifest_field(
		&mut document,
		"workspace.dependencies.core",
		None,
		None,
		&versions,
	);
	crate::update_manifest_field(
		&mut document,
		"workspace.dependencies.core.version",
		None,
		None,
		&versions,
	);
	crate::update_manifest_field(&mut document, "unknown.field", None, None, &versions);

	assert_eq!(
		document
			.get("package")
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("version"))
			.and_then(Item::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		document
			.get("dependencies")
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("core"))
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("version"))
			.and_then(Item::as_str),
		Some("2.0.0")
	);
	assert_eq!(
		document
			.get("workspace")
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("dependencies"))
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("core"))
			.and_then(Item::as_table_like)
			.and_then(|table| table.get("version"))
			.and_then(Item::as_str),
		Some("2.0.0")
	);

	let mut empty_document = ""
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("parse empty document: {error}"));
	crate::update_manifest_field(
		&mut empty_document,
		"dependencies.missing",
		None,
		None,
		&BTreeMap::new(),
	);
	crate::update_manifest_field(
		&mut empty_document,
		"dependencies.missing.version",
		None,
		None,
		&BTreeMap::new(),
	);
	crate::update_manifest_field(
		&mut empty_document,
		"workspace.dependencies.missing",
		None,
		None,
		&BTreeMap::new(),
	);
	crate::update_manifest_field(
		&mut empty_document,
		"workspace.dependencies.missing.version",
		None,
		None,
		&BTreeMap::new(),
	);
	crate::update_manifest_field(
		&mut empty_document,
		"dependencies.core",
		None,
		None,
		&versions,
	);
	crate::update_manifest_field(
		&mut empty_document,
		"dependencies.core.version",
		None,
		None,
		&versions,
	);
	crate::update_manifest_field(
		&mut empty_document,
		"workspace.dependencies.core",
		None,
		None,
		&versions,
	);

	crate::update_manifest_field(
		&mut empty_document,
		"workspace.dependencies.core.version",
		None,
		None,
		&versions,
	);

	assert!(crate::fields_target_workspace_dependencies(&[
		"workspace.dependencies"
	]));
	assert!(!crate::fields_target_workspace_dependencies(&[
		"dependencies"
	]));

	let mut helpers_document = "[dependencies]\n"
		.parse::<DocumentMut>()
		.unwrap_or_else(|error| panic!("parse helpers document: {error}"));
	let helpers_table = helpers_document
		.get_mut("dependencies")
		.and_then(Item::as_table_like_mut)
		.unwrap_or_else(|| panic!("expected helpers dependency table"));
	crate::update_dependency_by_name(helpers_table, "missing", "2.0.0");
	crate::update_dependency_version_by_name(helpers_table, "missing", "2.0.0");
}

#[test]
fn cargo_manifest_helpers_cover_workspace_and_error_paths() {
	let versioned_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/cargo/workspace-versioned");
	let root_manifest = versioned_root.join("Cargo.toml");
	assert!(has_workspace_section(&root_manifest).unwrap());
	let parsed = toml::from_str::<Value>(
		&fs::read_to_string(&root_manifest)
			.unwrap_or_else(|error| panic!("read workspace manifest: {error}")),
	)
	.unwrap_or_else(|error| panic!("parse workspace manifest: {error}"));
	assert_eq!(
		workspace_package_version(&parsed)
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("2.3.4")
	);

	let virtual_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/cargo/virtual-manifest");
	let virtual_manifest = virtual_root.join("Cargo.toml");
	assert_eq!(
		parse_package_manifest(&virtual_manifest, &virtual_root, None)
			.unwrap_or_else(|error| panic!("parse virtual manifest: {error}")),
		None
	);

	let invalid_workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/invalid-workspace/invalid-workspace.toml");
	let invalid_workspace_error = has_workspace_section(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected invalid workspace error"));
	assert!(
		invalid_workspace_error
			.to_string()
			.contains("failed to parse")
	);

	let invalid_package_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/invalid-package-name");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("create temp package dir: {error}"));
	fs::copy(
		invalid_package_root.join("invalid-workspace.toml"),
		tempdir.path().join("invalid-workspace.toml"),
	)
	.unwrap_or_else(|error| panic!("copy workspace manifest: {error}"));
	fs::copy(
		invalid_package_root.join("crates/core/invalid-package.toml"),
		tempdir.path().join("crates/core/Cargo.toml"),
	)
	.unwrap_or_else(|error| panic!("copy package manifest: {error}"));
	let discovery_error =
		discover_workspace_packages(&tempdir.path().join("invalid-workspace.toml"))
			.err()
			.unwrap_or_else(|| panic!("expected invalid package discovery error"));
	assert!(discovery_error.to_string().contains("missing package.name"));
}

#[test]
fn validate_versioned_file_treats_empty_custom_fields_as_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("Cargo.toml");
	fs::write(&path, "[package]\nname = \"test\"\n")
		.unwrap_or_else(|error| panic!("write: {error}"));
	// An explicitly empty field list falls back to the default version search.
	let result = super::validate_versioned_file(&path, "Cargo.toml", Some(&[]));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("does not contain a readable version field")
	);
}
