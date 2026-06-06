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
use super::apply_versioned_file_definition;
use super::build_versioned_file_updates_with_base_updates;
use super::inferred_lockfile_ecosystem_type;
use super::inferred_lockfile_paths;
use super::read_cached_document;
use super::released_versions_by_package_id;
use super::released_versions_by_record_id;
use super::seed_cached_text_updates;
use super::update_format_versioned_file_text;
use super::versioned_file_kind;

fn fixture_path(relative: &str) -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn npm_package_record(root: &Path, config_id: &str) -> monochange_core::PackageRecord {
	let manifest_path = root.join("packages/app/package.json");
	let current_version = "1.0.0"
		.parse()
		.unwrap_or_else(|error| panic!("current package version: {error}"));
	let mut package = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"@example/app",
		manifest_path,
		root.to_path_buf(),
		Some(current_version),
		monochange_core::PublishState::Public,
	);

	package
		.metadata
		.insert("config_id".to_string(), config_id.to_string());
	package
}

fn package_release_plan(
	root: &Path,
	package: &monochange_core::PackageRecord,
) -> monochange_core::ReleasePlan {
	monochange_core::ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![monochange_core::ReleaseDecision {
			package_id: package.id.clone(),
			trigger_type: "changeset".to_string(),
			recommended_bump: monochange_core::BumpSeverity::Minor,
			planned_version: Some(
				"1.2.3"
					.parse()
					.unwrap_or_else(|error| panic!("planned package version: {error}")),
			),
			group_id: None,
			reasons: Vec::new(),
			upstream_sources: Vec::new(),
			warnings: Vec::new(),
		}],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	}
}

fn snapshot_file_updates(root: &Path, updates: Vec<FileUpdate>) -> String {
	updates
		.into_iter()
		.map(|update| {
			let path = update
				.path
				.strip_prefix(root)
				.unwrap_or(&update.path)
				.to_string_lossy();
			let content = String::from_utf8(update.content)
				.unwrap_or_else(|error| panic!("updated file is utf-8: {error}"));

			format!("## {path}\n{content}")
		})
		.collect::<Vec<_>>()
		.join("\n---\n")
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
fn format_versioned_file_renders_name_and_version_templates_in_fields() {
	let json = update_format_versioned_file_text(
		"{\"packages\":{\"app\":{\"version\":\"1.0.0\"}}}",
		monochange_core::VersionedFileFormat::Json,
		&["packages.{{ name }}.version".to_string()],
		"1.2.3",
		Some("app"),
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("update templated json field: {error}"));
	assert!(json.contains("\"version\": \"1.2.3\""));
}

#[test]
fn format_versioned_file_ignores_missing_fields_by_default() {
	let json = update_format_versioned_file_text(
		"{\"release\":{\"version\":\"1.0.0\"}}",
		monochange_core::VersionedFileFormat::Json,
		&["release.missing".to_string()],
		"1.2.3",
		None,
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("missing json field should be ignored: {error}"));
	assert!(json.contains("\"version\": \"1.0.0\""));
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

	insta::assert_snapshot!(json, @r###"
	{
	  "versions": {
	    "app": "1.0.0",
	    "utils": "1.2.3"
	  }
	}
	"###);

	let toml = update_format_versioned_file_text(
		"[versions]\napp = \"1.0.0\"\n",
		monochange_core::VersionedFileFormat::Toml,
		&["versions.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing toml field: {error}"));
	insta::assert_snapshot!(toml, @r###"
	[versions]
	app = "1.0.0"
	utils = "1.2.3"
	"###);

	let yaml = update_format_versioned_file_text(
		"versions:\n  app: 1.0.0\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["versions.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing yaml field: {error}"));
	insta::assert_snapshot!(yaml, @r###"
	versions:
	  app: 1.0.0
	  utils: 1.2.3
	"###);

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
	insta::assert_snapshot!(json_with_nested_parent, @r###"
	{
	  "versions": {
	    "packages": {
	      "utils": "1.2.3"
	    }
	  }
	}
	"###);

	let toml_with_nested_parent = update_format_versioned_file_text(
		"",
		monochange_core::VersionedFileFormat::Toml,
		&["versions.packages.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing nested toml field: {error}"));
	insta::assert_snapshot!(toml_with_nested_parent, @r###"
	[versions]

	[versions.packages]
	utils = "1.2.3"
	"###);

	let yaml_with_nested_parent = update_format_versioned_file_text(
		"{}\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["versions.packages.{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::Add,
	)
	.unwrap_or_else(|error| panic!("add missing nested yaml field: {error}"));
	insta::assert_snapshot!(yaml_with_nested_parent, @r###"
	versions:
	  packages:
	    utils: 1.2.3
	"###);
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
fn format_versioned_file_skips_missing_rendered_name_fields() {
	let json = update_format_versioned_file_text(
		"{\"app\":\"1.0.0\"}",
		monochange_core::VersionedFileFormat::Json,
		&["{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("skip missing json name field: {error}"));
	assert!(json.contains("\"app\": \"1.0.0\""));

	let toml = update_format_versioned_file_text(
		"app = \"1.0.0\"\n",
		monochange_core::VersionedFileFormat::Toml,
		&["{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("skip missing toml name field: {error}"));
	assert_eq!(toml, "app = \"1.0.0\"\n");

	let yaml = update_format_versioned_file_text(
		"app: 1.0.0\n",
		monochange_core::VersionedFileFormat::Yaml,
		&["{{ name }}".to_string()],
		"1.2.3",
		Some("utils"),
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("skip missing yaml name field: {error}"));
	assert!(yaml.contains("app: 1.0.0"));

	let env = update_format_versioned_file_text(
		"APP_VERSION=1.0.0\n",
		monochange_core::VersionedFileFormat::Env,
		&["{{ name }}".to_string()],
		"1.2.3",
		Some("UTILS_VERSION"),
		monochange_core::MissingFieldBehavior::default(),
	)
	.unwrap_or_else(|error| panic!("skip missing env name field: {error}"));
	assert_eq!(env, "APP_VERSION=1.0.0\n");
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
fn build_versioned_file_updates_returns_empty_for_empty_configuration() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"npm\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange config: {error}"));
	let configuration = monochange_config::load_workspace_configuration(root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let plan = monochange_core::ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: Vec::new(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let updates =
		build_versioned_file_updates_with_base_updates(root, &configuration, &[], &plan, &[])
			.unwrap_or_else(|error| panic!("build versioned updates: {error}"));

	assert!(updates.is_empty());
}

#[test]
fn build_versioned_file_updates_uses_default_versioned_files() {
	let root = fixture_path("versioned-files-defaults");
	let configuration = monochange_config::load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = npm_package_record(&root, "app");
	let plan = package_release_plan(&root, &package);

	let updates = build_versioned_file_updates_with_base_updates(
		&root,
		&configuration,
		&[package],
		&plan,
		&[],
	)
	.unwrap_or_else(|error| panic!("build versioned updates: {error}"));

	insta::assert_snapshot!(snapshot_file_updates(&root, updates));
}

#[test]
fn build_versioned_file_updates_uses_ecosystem_versioned_files() {
	let root = fixture_path("versioned-files-ecosystems");
	let configuration = monochange_config::load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = npm_package_record(&root, "app");
	let plan = package_release_plan(&root, &package);

	let updates = build_versioned_file_updates_with_base_updates(
		&root,
		&configuration,
		&[package],
		&plan,
		&[],
	)
	.unwrap_or_else(|error| panic!("build versioned updates: {error}"));

	insta::assert_snapshot!(snapshot_file_updates(&root, updates));
}

#[test]
fn seed_cached_text_updates_rejects_invalid_utf8_and_resolves_relative_paths() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let relative_update = FileUpdate {
		path: PathBuf::from("pubspec.yaml"),
		content: b"name: app\n".to_vec(),
	};
	let mut updates = BTreeMap::new();

	seed_cached_text_updates(root, &mut updates, &[relative_update])
		.unwrap_or_else(|error| panic!("seed relative update: {error}"));

	assert!(matches!(
		updates.get(&root.join("pubspec.yaml")),
		Some(CachedDocument::Text(contents)) if contents == "name: app\n"
	));

	let invalid_update = FileUpdate {
		path: PathBuf::from("invalid.yaml"),
		content: vec![0xff, 0xfe],
	};
	let error = seed_cached_text_updates(root, &mut updates, &[invalid_update])
		.expect_err("invalid seeded text update should fail");

	assert!(
		error
			.to_string()
			.contains("failed to parse invalid.yaml as text")
	);
}

#[test]
fn released_version_maps_skip_unplanned_groups() {
	let plan = monochange_core::ReleasePlan {
		workspace_root: PathBuf::from("/workspace"),
		decisions: Vec::new(),
		groups: vec![monochange_core::PlannedVersionGroup {
			group_id: "sdk".to_string(),
			display_name: "SDK".to_string(),
			members: vec!["core".to_string()],
			mismatch_detected: false,
			planned_version: None,
			recommended_bump: monochange_core::BumpSeverity::None,
		}],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	assert!(released_versions_by_record_id(&plan).is_empty());
	assert!(released_versions_by_package_id(&plan, &[]).is_empty());
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
fn read_cached_document_handles_go_text_and_invalid_utf8() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let go_mod = tempdir.path().join("go.mod");
	std::fs::write(&go_mod, "module github.com/example/repo\n")
		.unwrap_or_else(|error| panic!("write go.mod: {error}"));
	let mut updates = BTreeMap::new();

	let document = read_cached_document(&mut updates, &go_mod, EcosystemType::Go)
		.unwrap_or_else(|error| panic!("go text document: {error}"));
	assert!(matches!(document, CachedDocument::Text(_)));

	std::fs::write(&go_mod, [0xff, 0xfe])
		.unwrap_or_else(|error| panic!("write invalid go.mod: {error}"));
	let error = read_cached_document(&mut updates, &go_mod, EcosystemType::Go)
		.expect_err("invalid go.mod should fail");
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
#[cfg(feature = "npm")]
fn read_cached_document_preserves_bun_lock_binary_without_utf8_parse() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let bun_lock = tempdir.path().join("bun.lockb");
	let binary_contents = vec![0xff, 0xfe, 0x00, 0x01];
	std::fs::write(&bun_lock, &binary_contents)
		.unwrap_or_else(|error| panic!("write bun.lockb: {error}"));
	let mut updates = BTreeMap::new();

	let document = read_cached_document(&mut updates, &bun_lock, EcosystemType::Npm)
		.unwrap_or_else(|error| panic!("bun lock binary document: {error}"));

	assert!(matches!(document, CachedDocument::Bytes(contents) if contents == binary_contents));
}

#[test]
fn read_cached_document_reports_deno_lock_invalid_utf8_as_text_parse_error() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let deno_lock = tempdir.path().join("deno.lock");
	std::fs::write(&deno_lock, [0xff, 0xfe])
		.unwrap_or_else(|error| panic!("write invalid deno.lock: {error}"));
	let mut updates = BTreeMap::new();

	let error = read_cached_document(&mut updates, &deno_lock, EcosystemType::Deno)
		.expect_err("invalid deno.lock should fail");
	let message = error.to_string();
	assert!(message.contains("failed to parse"));
	assert!(message.contains("as text"));
}

#[test]
fn read_cached_document_reports_go_for_unsupported_go_versioned_file() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let notes = tempdir.path().join("notes.txt");
	std::fs::write(&notes, "version = 1.0.0\n")
		.unwrap_or_else(|error| panic!("write notes: {error}"));
	let mut updates = BTreeMap::new();

	let error = read_cached_document(&mut updates, &notes, EcosystemType::Go)
		.expect_err("unsupported go versioned file");

	assert!(error.to_string().contains("ecosystem `go`"));
}

#[test]
fn apply_versioned_file_definition_reports_go_for_unsupported_glob_match() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(tempdir.path().join("notes.txt"), "version = 1.0.0\n")
		.unwrap_or_else(|error| panic!("write notes: {error}"));
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
	};
	let definition = monochange_core::VersionedFileDefinition {
		path: "*.txt".to_string(),
		ecosystem_type: Some(EcosystemType::Go),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
	};
	let mut updates = BTreeMap::new();

	let error = apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&definition,
		"1.2.3",
		None,
		&["lib".to_string()],
		&context,
	)
	.expect_err("unsupported go glob match");

	assert!(error.to_string().contains("ecosystem `go`"));
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
	)
	.unwrap_or_else(|error| panic!("build versioned updates: {error}"));

	assert_eq!(updates.len(), 1);
	assert_eq!(updates[0].path, manifest_path);
	let content = String::from_utf8(updates[0].content.clone())
		.unwrap_or_else(|error| panic!("updated manifest should be utf-8: {error}"));
	assert!(content.contains(r#""lib": "1.2.0""#));
	assert!(content.contains(r#""actual": "^1.0.0""#));

	std::fs::write(&manifest_path, "{")
		.unwrap_or_else(|error| panic!("write invalid package manifest: {error}"));
	let error = build_versioned_file_updates_with_base_updates(
		root,
		&configuration,
		&[package],
		&plan,
		&[],
	)
	.expect_err("invalid overridden versioned file should fail");
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn inferred_lockfile_ecosystem_type_maps_python_when_commands_are_not_configured() {
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("monochange/release-base"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(
		inferred_lockfile_ecosystem_type(&configuration, Ecosystem::Python),
		Some(EcosystemType::Python)
	);
}
