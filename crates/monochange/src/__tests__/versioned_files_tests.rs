#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::EcosystemType;

use super::CachedDocument;
use super::FileUpdate;
use super::VersionedFileUpdateContext;
use super::add_json_field_path;
use super::add_toml_field_path;
use super::add_yaml_field_path;
use super::apply_versioned_file_definition_to_paths;
use super::build_versioned_file_updates_with_base_updates;
use super::inferred_lockfile_ecosystem_type;
use super::inferred_lockfile_paths;
use super::read_cached_document;
use super::released_versions_by_record_id;
use super::seed_cached_text_updates;
use super::update_format_versioned_file_text;
use super::versioned_file_kind;

fn apply_versioned_file_definition(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, super::CachedDocument>,
	definition: &monochange_core::VersionedFileDefinition,
	owner_version: &str,
	shared_release_version: Option<&String>,
	dep_names: &[impl AsRef<str>],
	context: &super::VersionedFileUpdateContext<'_>,
) -> monochange_core::MonochangeResult<()> {
	let resolved_paths = monochange_core::workspace_glob_files(root, &definition.path)?;
	super::apply_versioned_file_definition_to_paths(
		root,
		updates,
		definition,
		&super::VersionedFileSite {
			resolved_paths: &resolved_paths,
			owner_version,
			shared_release_version,
			dep_names,
		},
		context,
	)
}

fn fixture_path(relative: &str) -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

#[test]
fn format_versioned_file_updates_json_toml_yaml_and_env_fields() {
	let json = update_format_versioned_file_text(
		"{\"release\":{\"version\":\"1.0.0\"}}",
		monochange_core::VersionedFileFormat::Json,
		&["release.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("update json: {error}"));
	assert!(json.contains("\"version\": \"1.2.3\""));

	let toml = update_format_versioned_file_text(
		"[tool.app]\nversion = \"1.0.0\"\n",
		monochange_core::VersionedFileFormat::Toml,
		&["tool.app.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("update toml: {error}"));
	assert!(toml.contains("version = \"1.2.3\""));

	let yaml = update_format_versioned_file_text(
		"release:\n  version: 1.0.0\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["release.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("update yaml: {error}"));
	assert!(yaml.contains("version: 1.2.3"));

	let env = update_format_versioned_file_text(
		"APP=demo\nexport VERSION=1.0.0\n",
		monochange_core::VersionedFileFormat::Env,
		&["VERSION".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("update env: {error}"));
	assert_eq!(env, "APP=demo\nexport VERSION=1.2.3\n");
}

#[test]
fn format_versioned_file_adds_missing_fields_when_configured() {
	let json = update_format_versioned_file_text(
		"{\"versions\":{\"app\":\"1.0.0\"}}",
		monochange_core::VersionedFileFormat::Json,
		&["versions.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing json field: {error}"));

	insta::assert_snapshot!(json, @r#"
	{
	  "versions": {
	    "app": "1.0.0",
	    "utils": "1.2.3"
	  }
	}
	"#);

	let toml = update_format_versioned_file_text(
		"[versions]\napp = \"1.0.0\"\n",
		monochange_core::VersionedFileFormat::Toml,
		&["versions.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing toml field: {error}"));
	insta::assert_snapshot!(toml, @r#"
	[versions]
	app = "1.0.0"
	utils = "1.2.3"
	"#);

	let yaml = update_format_versioned_file_text(
		"versions:\n  app: 1.0.0\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["versions.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing yaml field: {error}"));
	insta::assert_snapshot!(yaml, @"
	versions:
	  app: 1.0.0
	  utils: 1.2.3
	");

	let env = update_format_versioned_file_text(
		"APP=demo\n",
		monochange_core::VersionedFileFormat::Env,
		&["VERSION".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing env key: {error}"));
	assert_eq!(env, "APP=demo\nVERSION=1.2.3\n");

	let env_without_newline = update_format_versioned_file_text(
		"APP=demo",
		monochange_core::VersionedFileFormat::Env,
		&["VERSION".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing env key after unterminated line: {error}"));
	assert_eq!(env_without_newline, "APP=demo\nVERSION=1.2.3\n");

	let json_with_nested_parent = update_format_versioned_file_text(
		"{}",
		monochange_core::VersionedFileFormat::Json,
		&["versions.packages.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing nested json field: {error}"));
	insta::assert_snapshot!(json_with_nested_parent, @r#"
	{
	  "versions": {
	    "packages": {
	      "utils": "1.2.3"
	    }
	  }
	}
	"#);

	let toml_with_nested_parent = update_format_versioned_file_text(
		"",
		monochange_core::VersionedFileFormat::Toml,
		&["versions.packages.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing nested toml field: {error}"));
	insta::assert_snapshot!(toml_with_nested_parent, @r#"
	[versions]

	[versions.packages]
	utils = "1.2.3"
	"#);

	let yaml_with_nested_parent = update_format_versioned_file_text(
		"{}\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["versions.packages.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing nested yaml field: {error}"));
	insta::assert_snapshot!(yaml_with_nested_parent, @"
	versions:
	  packages:
	    utils: 1.2.3
	");
}

#[test]
fn add_missing_field_helpers_reject_non_container_values() {
	let mut json = serde_json::json!("1.0.0");
	let json_error = add_json_field_path(&mut json, "version", "1.2.3")
		.expect_err("json scalar root cannot receive a field");
	assert!(json_error.to_string().contains("non-object value"));

	let mut nested_json_root = serde_json::json!("1.0.0");
	let nested_json_root_error =
		add_json_field_path(&mut nested_json_root, "versions.app", "1.2.3")
			.expect_err("json scalar root cannot receive a nested field");
	assert!(
		nested_json_root_error
			.to_string()
			.contains("non-object segment `versions`")
	);

	let mut nested_json = serde_json::json!({ "versions": "1.0.0" });
	let nested_json_error = add_json_field_path(&mut nested_json, "versions.app", "1.2.3")
		.expect_err("json scalar parent cannot receive a child field");
	assert!(nested_json_error.to_string().contains("non-object value"));

	let mut toml = "[tool]\napp = \"1.0.0\"\n"
		.parse::<toml_edit::DocumentMut>()
		.unwrap_or_else(|error| panic!("parse toml: {error}"));
	let toml_error = add_toml_field_path(&mut toml, "tool.app.version", "1.2.3")
		.expect_err("toml scalar segment cannot receive a child field");
	assert!(toml_error.to_string().contains("non-table segment `app`"));

	let mut yaml = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("1.0.0\n")
		.unwrap_or_else(|error| panic!("parse yaml: {error}"));
	let yaml_error = add_yaml_field_path(&mut yaml, "version", "1.2.3")
		.expect_err("yaml scalar root cannot receive a field");
	assert!(yaml_error.to_string().contains("non-mapping value"));

	let mut nested_yaml_root = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("1.0.0\n")
		.unwrap_or_else(|error| panic!("parse nested yaml root: {error}"));
	let nested_yaml_root_error =
		add_yaml_field_path(&mut nested_yaml_root, "versions.app", "1.2.3")
			.expect_err("yaml scalar root cannot receive a nested field");
	assert!(
		nested_yaml_root_error
			.to_string()
			.contains("non-mapping segment `versions`")
	);

	let mut nested_yaml = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("versions: 1.0.0\n")
		.unwrap_or_else(|error| panic!("parse nested yaml: {error}"));
	let nested_yaml_error = add_yaml_field_path(&mut nested_yaml, "versions.app", "1.2.3")
		.expect_err("yaml scalar parent cannot receive a child field");
	assert!(nested_yaml_error.to_string().contains("non-mapping value"));
}

#[test]
fn format_versioned_file_reports_invalid_paths_and_parse_errors() {
	let json_non_object = update_format_versioned_file_text(
		"{\"release\":\"1.0.0\"}",
		monochange_core::VersionedFileFormat::Json,
		&["release.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("json traversal through a string should fail");
	assert!(json_non_object.to_string().contains("non-object value"));

	let json_non_object_segment = update_format_versioned_file_text(
		"{\"release\":{\"metadata\":\"1.0.0\"}}",
		monochange_core::VersionedFileFormat::Json,
		&["release.metadata.inner.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("json traversal after a string segment should fail");
	assert!(
		json_non_object_segment
			.to_string()
			.contains("non-object segment `inner`")
	);

	let json_root_scalar = update_format_versioned_file_text(
		"\"1.0.0\"",
		monochange_core::VersionedFileFormat::Json,
		&["version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("json scalar root should fail");
	assert!(json_root_scalar.to_string().contains("non-object value"));

	let json_parse = update_format_versioned_file_text(
		"{",
		monochange_core::VersionedFileFormat::Json,
		&["version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("invalid json should fail");
	assert!(json_parse.to_string().contains("failed to parse json"));

	let toml_non_table = update_format_versioned_file_text(
		"[tool]\napp = \"1.0.0\"\n",
		monochange_core::VersionedFileFormat::Toml,
		&["tool.app.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("toml traversal through a string should fail");
	assert!(
		toml_non_table
			.to_string()
			.contains("non-table segment `app`")
	);

	let toml_parse = update_format_versioned_file_text(
		"[tool",
		monochange_core::VersionedFileFormat::Toml,
		&["tool.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("invalid toml should fail");
	assert!(toml_parse.to_string().contains("failed to parse toml"));

	let yaml_non_mapping = update_format_versioned_file_text(
		"release: 1.0.0\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["release.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("yaml traversal through a scalar should fail");
	assert!(yaml_non_mapping.to_string().contains("non-mapping value"));

	let yaml_non_mapping_segment = update_format_versioned_file_text(
		"release:\n  metadata: 1.0.0\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["release.metadata.inner.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("yaml traversal after a scalar segment should fail");
	assert!(
		yaml_non_mapping_segment
			.to_string()
			.contains("non-mapping segment `inner`")
	);

	let yaml_root_scalar = update_format_versioned_file_text(
		"1.0.0\n",
		monochange_core::VersionedFileFormat::Yml,
		&["version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("yaml scalar root should fail");
	assert!(yaml_root_scalar.to_string().contains("non-mapping value"));

	let yaml_parse = update_format_versioned_file_text(
		"release: [",
		monochange_core::VersionedFileFormat::Yaml,
		&["release.version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("invalid yaml should fail");
	assert!(yaml_parse.to_string().contains("failed to parse yaml"));

	let empty_segment = update_format_versioned_file_text(
		"{\"release\":{\"version\":\"1.0.0\"}}",
		monochange_core::VersionedFileFormat::Json,
		&["release..version".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.expect_err("empty path segment should fail");
	assert!(
		empty_segment
			.to_string()
			.contains("non-empty dot-separated segments")
	);
}

#[test]
fn apply_versioned_file_definition_supports_format_mode_and_reports_format_errors() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::fs::write(
		root.join("metadata.json"),
		"{\"release\":{\"version\":\"1.0.0\"}}",
	)
	.unwrap_or_else(|error| panic!("write metadata: {error}"));
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let context = VersionedFileUpdateContext {
		package_by_config_id: BTreeMap::new(),
		package_by_native_name: BTreeMap::new(),
		current_versions_by_native_name: BTreeMap::new(),
		released_versions_by_native_name: BTreeMap::new(),
		configuration: &configuration,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		release_values: BTreeMap::new(),
	};
	let definition = monochange_core::VersionedFileDefinition {
		path: "*.json".to_string(),
		ecosystem_type: None,
		format: Some(monochange_core::VersionedFileFormat::Json),
		prefix: None,
		fields: Some(vec!["release.version".to_string()]),
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let mut updates = BTreeMap::new();

	apply_versioned_file_definition(
		root,
		&mut updates,
		&definition,
		"1.2.3",
		None,
		&["metadata".to_string()],
		&context,
	)
	.unwrap_or_else(|error| panic!("apply format definition: {error}"));
	assert!(matches!(
		updates.get(&root.join("metadata.json")),
		Some(CachedDocument::Text(contents)) if contents.contains("\"version\": \"1.2.3\"")
	));

	std::fs::write(root.join("invalid.json"), "{")
		.unwrap_or_else(|error| panic!("write invalid json: {error}"));
	let invalid_json = monochange_core::VersionedFileDefinition {
		path: "invalid.json".to_string(),
		..definition.clone()
	};
	let error = apply_versioned_file_definition(
		root,
		&mut updates,
		&invalid_json,
		"1.2.3",
		None,
		&["invalid".to_string()],
		&context,
	)
	.expect_err("invalid formatted file should fail through apply");
	assert!(error.to_string().contains("failed to parse json"));

	let missing_fields = monochange_core::VersionedFileDefinition {
		fields: None,
		..definition.clone()
	};
	let error = apply_versioned_file_definition(
		root,
		&mut updates,
		&missing_fields,
		"1.2.3",
		None,
		&["metadata".to_string()],
		&context,
	)
	.expect_err("format definition without fields should fail");
	assert!(
		error
			.to_string()
			.contains("with format mode is missing fields")
	);

	let invalid_glob = monochange_core::VersionedFileDefinition {
		path: "[".to_string(),
		..definition
	};
	let error = apply_versioned_file_definition(
		root,
		&mut updates,
		&invalid_glob,
		"1.2.3",
		None,
		&["metadata".to_string()],
		&context,
	)
	.expect_err("invalid format glob should fail");
	assert!(error.to_string().contains("invalid glob pattern"));
}

#[test]
fn go_versioned_file_kind_and_lockfile_inference_are_supported() {
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let module_dir = tempdir.path().join("api");
	std::fs::create_dir(&module_dir)
		.unwrap_or_else(|error| panic!("create api module dir: {error}"));
	std::fs::write(module_dir.join("go.sum"), "")
		.unwrap_or_else(|error| panic!("write go.sum: {error}"));
	let package = monochange_core::PackageRecord {
		ecosystem: Ecosystem::Go,
		manifest_path: module_dir.join("go.mod"),
		..monochange_core::PackageRecord::new(
			Ecosystem::Go,
			"github.com/example/repo/api",
			module_dir.join("go.mod"),
			tempdir.path().to_path_buf(),
			None,
			monochange_core::PublishState::Public,
		)
	};

	assert!(versioned_file_kind(EcosystemType::Go, Path::new("go.mod")).is_some());
	assert_eq!(
		inferred_lockfile_ecosystem_type(&configuration, Ecosystem::Go),
		Some(EcosystemType::Go)
	);
	assert_eq!(
		inferred_lockfile_paths(&package),
		vec![module_dir.join("go.sum")]
	);
}

#[test]
fn apply_versioned_file_definition_updates_go_mod_dependencies() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let go_mod = tempdir.path().join("go.mod");
	std::fs::write(
		&go_mod,
		"module github.com/example/app\n\ngo 1.22\n\nrequire github.com/example/lib v1.0.0\n",
	)
	.unwrap_or_else(|error| panic!("write go.mod: {error}"));
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut released_versions = BTreeMap::new();
	released_versions.insert("lib".to_string(), "1.2.3".to_string());
	let context = VersionedFileUpdateContext {
		package_by_config_id: BTreeMap::new(),
		package_by_native_name: BTreeMap::new(),
		current_versions_by_native_name: BTreeMap::new(),
		released_versions_by_native_name: released_versions,
		configuration: &configuration,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		release_values: BTreeMap::new(),
	};

	let definition = monochange_core::VersionedFileDefinition {
		path: "go.mod".to_string(),
		ecosystem_type: Some(EcosystemType::Go),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let mut updates = BTreeMap::new();

	apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"1.2.3",
		None,
		&["lib".to_string()],
		&context,
	)
	.unwrap_or_else(|error| panic!("apply go update: {error}"));
	let updated_document = updates
		.into_values()
		.next()
		.unwrap_or_else(|| panic!("updated go.mod"));
	assert!(matches!(
		updated_document,
		CachedDocument::Text(contents) if contents.contains("github.com/example/lib v1.2.3")
	));
}

#[test]
fn build_versioned_file_updates_uses_package_versioned_file_name_override() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let package_dir = root.join("packages/lib");
	std::fs::create_dir_all(&package_dir)
		.unwrap_or_else(|error| panic!("create package directory: {error}"));
	let manifest_path = package_dir.join("package.json");
	std::fs::write(
		&manifest_path,
		r#"{
  "name": "consumer",
  "version": "0.0.0",
  "dependencies": {
    "lib": "^1.0.0",
    "actual": "^1.0.0"
  }
}
"#,
	)
	.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	std::fs::write(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "npm"

[package.lib]
path = "packages/lib"
versioned_files = [
  { path = "packages/lib/package.json", type = "npm", name = "lib", fields = ["dependencies.{{ name }}"] }
]

[ecosystems.npm]
enabled = true
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange config: {error}"));
	let configuration = monochange_config::load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut package = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"lib",
		manifest_path.clone(),
		root.to_path_buf(),
		Some(semver::Version::new(1, 0, 0)),
		monochange_core::PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), "lib".to_string());
	let plan = monochange_core::ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: package.id.clone(),
			trigger_type: "changeset".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Minor,
			planned_version: Some(semver::Version::new(1, 2, 0)),
			group_id: None,
			reasons: Vec::new(),
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let updates = build_versioned_file_updates_with_base_updates(
		root,
		&configuration,
		std::slice::from_ref(&package),
		&plan,
		&[],
		&crate::versioning_state::ResolvedReleaseValues::default(),
	)
	.unwrap_or_else(|error| panic!("build versioned updates: {error}"));

	assert_eq!(updates.len(), 1);
	assert_eq!(updates[0].path, manifest_path);
	let content = String::from_utf8(updates[0].content.clone())
		.unwrap_or_else(|error| panic!("updated manifest should be utf-8: {error}"));
	assert!(content.contains(r#""lib": "^1.2.0""#));
	assert!(content.contains(r#""actual": "^1.0.0""#));

	std::fs::write(&manifest_path, "{")
		.unwrap_or_else(|error| panic!("write invalid package manifest: {error}"));
	let error = build_versioned_file_updates_with_base_updates(
		root,
		&configuration,
		&[package],
		&plan,
		&[],
		&crate::versioning_state::ResolvedReleaseValues::default(),
	)
	.expect_err("invalid overridden versioned file should fail");
	assert!(error.to_string().contains("failed to parse"));
}

// -- native ecosystem versioned files --
fn native_context<'a>(
	configuration: &'a monochange_core::WorkspaceConfiguration,
	released: &[(&'a str, &str)],
) -> VersionedFileUpdateContext<'a> {
	VersionedFileUpdateContext {
		package_by_config_id: BTreeMap::new(),
		package_by_native_name: BTreeMap::new(),
		current_versions_by_native_name: BTreeMap::new(),
		released_versions_by_native_name: released
			.iter()
			.map(|(name, version)| ((*name).to_string(), (*version).to_string()))
			.collect(),
		configuration,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		release_values: BTreeMap::new(),
	}
}

#[test]
fn podspec_versioned_file_updates_the_declared_version() {
	let root = fixture_path("versioned-files/native");
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let context = native_context(&configuration, &[("Canvist", "0.2.0")]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Canvist.podspec".to_string(),
		ecosystem_type: Some(EcosystemType::CocoaPods),
		format: None,
		prefix: None,
		fields: Some(vec!["version".to_string()]),
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let mut updates = BTreeMap::new();

	apply_versioned_file_definition(
		&root,
		&mut updates,
		&definition,
		"0.2.0",
		None,
		&["Canvist"],
		&context,
	)
	.unwrap_or_else(|error| panic!("apply podspec update: {error}"));

	let super::CachedDocument::Text(contents) = updates
		.remove(&root.join("Canvist.podspec"))
		.unwrap_or_else(|| panic!("expected podspec update"))
	else {
		panic!("expected podspec text document");
	};

	assert!(contents.contains("s.version = \"0.2.0\""), "{contents}");
	assert!(contents.contains("s.name = \"Canvist\""), "{contents}");
}

#[test]
fn gradle_properties_versioned_file_updates_version_name() {
	let root = fixture_path("versioned-files/native");
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let context = native_context(&configuration, &[("dev.canvist:canvist-android", "0.2.0")]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "gradle.properties".to_string(),
		ecosystem_type: Some(EcosystemType::Gradle),
		format: None,
		prefix: None,
		fields: Some(vec!["VERSION_NAME".to_string()]),
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let mut updates = BTreeMap::new();

	apply_versioned_file_definition(
		&root,
		&mut updates,
		&definition,
		"0.2.0",
		None,
		&["dev.canvist:canvist-android"],
		&context,
	)
	.unwrap_or_else(|error| panic!("apply gradle update: {error}"));

	let super::CachedDocument::Text(contents) = updates
		.remove(&root.join("gradle.properties"))
		.unwrap_or_else(|| panic!("expected gradle.properties update"))
	else {
		panic!("expected gradle.properties text document");
	};

	assert!(contents.contains("VERSION_NAME=0.2.0"), "{contents}");
	assert!(contents.contains("GROUP=dev.canvist"), "{contents}");
}

#[test]
fn native_cached_documents_reject_non_utf8_files() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::fs::write(
		root.join("Canvist.podspec"),
		include_bytes!("../../../../fixtures/tests/native-adapters/invalid-utf8"),
	)
	.unwrap_or_else(|error| panic!("write invalid podspec: {error}"));
	std::fs::write(
		root.join("gradle.properties"),
		include_bytes!("../../../../fixtures/tests/native-adapters/invalid-utf8"),
	)
	.unwrap_or_else(|error| panic!("write invalid gradle.properties: {error}"));
	std::fs::copy(root.join("gradle.properties"), root.join("Package.swift"))
		.expect("copy invalid Swift manifest");
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let context = native_context(&configuration, &[("Canvist", "0.2.0")]);

	for (ecosystem_type, path) in [
		(EcosystemType::CocoaPods, "Canvist.podspec"),
		(EcosystemType::Gradle, "gradle.properties"),
		(EcosystemType::Swift, "Package.swift"),
	] {
		let definition = monochange_core::VersionedFileDefinition {
			path: path.to_string(),
			ecosystem_type: Some(ecosystem_type),
			format: None,
			prefix: None,
			fields: Some(vec!["version".to_string()]),
			name: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		};
		let mut updates = BTreeMap::new();

		let error = apply_versioned_file_definition(
			root,
			&mut updates,
			&definition,
			"0.2.0",
			None,
			&["Canvist"],
			&context,
		)
		.expect_err("a non-UTF-8 native manifest must fail");

		assert!(
			error.to_string().contains("failed to parse"),
			"unexpected error: {error}"
		);
	}
}

#[test]
fn podspec_json_updates_report_invalid_json() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::fs::write(root.join("Canvist.podspec.json"), "{not json")
		.unwrap_or_else(|error| panic!("write invalid podspec.json: {error}"));
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	let context = native_context(&configuration, &[("Canvist", "0.2.0")]);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Canvist.podspec.json".to_string(),
		ecosystem_type: Some(EcosystemType::CocoaPods),
		format: None,
		prefix: None,
		fields: Some(vec!["version".to_string()]),
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let mut updates = BTreeMap::new();

	let error = apply_versioned_file_definition(
		root,
		&mut updates,
		&definition,
		"0.2.0",
		None,
		&["Canvist"],
		&context,
	)
	.expect_err("an invalid JSON podspec must fail");

	assert!(
		error.to_string().contains("failed to parse"),
		"unexpected error: {error}"
	);
}
