use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishState;
use monochange_core::PublishTimeoutSettings;
use monochange_core::RegistryKind;
use monochange_core::TrustedPublishingSettings;
use monochange_core::materialize_dependency_edges;
use monochange_github::GitHubTrustContext;
use monochange_publish::PublishRequest;
use semver::Version;
use serde_json::json;
use serde_yaml_ng::Value as YamlValue;
use tempfile::tempdir;

use crate::NpmVersionedFileKind;
use crate::adapter;
use crate::default_lockfile_commands;
use crate::detect_npm_manager;
use crate::discover_lockfiles;
use crate::discover_npm_packages;
use crate::discover_package_json_workspace;
use crate::discover_pnpm_workspace;
use crate::expand_member_patterns;
use crate::load_configured_npm_package;
use crate::package_json_declares_workspaces;
use crate::parse_package_json;
use crate::supported_versioned_file_kind;
use crate::update_bun_lock;
use crate::update_bun_lock_binary;
use crate::update_json_dependency_fields;
use crate::update_package_lock;
use crate::update_pnpm_lock;
use crate::update_pnpm_lock_text;
use crate::update_yarn_lock;
use crate::workspace_patterns_from_package_json;

#[test]
fn discovers_npm_workspace_packages() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "npm-web")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "npm-shared")
	);
	let dependency_edges = materialize_dependency_edges(&discovery.packages);
	assert_eq!(dependency_edges.len(), 1);
}

#[test]
fn discovers_pnpm_workspace_globs() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-pnpm");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("pnpm discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "pnpm-web")
	);
}

#[test]
fn discovers_bun_workspace_packages() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-bun");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("bun discovery: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	let web_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "bun-web")
		.unwrap_or_else(|| panic!("bun web package should exist"));
	assert_eq!(
		web_package.metadata.get("manager").map(String::as_str),
		Some("bun")
	);
}

#[test]
fn adapter_reports_npm_ecosystem() {
	assert_eq!(adapter().ecosystem(), Ecosystem::Npm);
}

#[test]
fn supported_versioned_file_kind_recognizes_known_files() {
	assert_eq!(
		supported_versioned_file_kind(Path::new("package.json")),
		Some(NpmVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("package-lock.json")),
		Some(NpmVersionedFileKind::PackageLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("pnpm-lock.yaml")),
		Some(NpmVersionedFileKind::PnpmLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("yarn.lock")),
		Some(NpmVersionedFileKind::YarnLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("bun.lock")),
		Some(NpmVersionedFileKind::BunLock)
	);
	assert_eq!(
		supported_versioned_file_kind(Path::new("bun.lockb")),
		Some(NpmVersionedFileKind::BunLockBinary)
	);
	assert_eq!(supported_versioned_file_kind(Path::new("README.md")), None);
}

#[test]
fn discover_lockfiles_prefers_workspace_root_then_manifest_directory() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"pnpm-web",
		fixture_root.join("packages/web/package.json"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("pnpm-lock.yaml")
		))
	);
}

#[test]
fn discover_lockfiles_discovers_yarn_lockfiles() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/yarn-lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"yarn-web",
		fixture_root.join("packages/web/package.json"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("yarn.lock")
		))
	);
}

#[test]
fn discover_lockfiles_falls_back_to_manifest_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/manifest-lockfile-workspace");
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"nested-web",
		fixture_root.join("packages/web/package.json"),
		fixture_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert_eq!(
		lockfiles.first(),
		Some(&monochange_core::normalize_path(
			&fixture_root.join("packages/web/package-lock.json")
		))
	);
}

#[test]
fn default_lockfile_commands_match_owned_npm_lockfile_kind() {
	let package_lock_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/manifest-lockfile-workspace");
	let package_lock_package = PackageRecord::new(
		Ecosystem::Npm,
		"nested-web",
		package_lock_root.join("packages/web/package.json"),
		package_lock_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&package_lock_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "npm install --package-lock-only".to_string(),
			cwd: monochange_core::normalize_path(&package_lock_root.join("packages/web")),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let pnpm_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/lockfile-workspace");
	let pnpm_package = PackageRecord::new(
		Ecosystem::Npm,
		"nested-web",
		pnpm_root.join("packages/web/package.json"),
		pnpm_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&pnpm_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "pnpm install --lockfile-only".to_string(),
			cwd: monochange_core::normalize_path(&pnpm_root),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let yarn_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/yarn-lockfile-workspace");
	let yarn_package = PackageRecord::new(
		Ecosystem::Npm,
		"yarn-web",
		yarn_root.join("packages/web/package.json"),
		yarn_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&yarn_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "yarn install --mode=update-lockfile".to_string(),
			cwd: monochange_core::normalize_path(&yarn_root),
			shell: monochange_core::ShellConfig::None,
		}]
	);

	let bun_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/monochange/bun-lock-release");
	let bun_package = PackageRecord::new(
		Ecosystem::Npm,
		"workflow-app",
		bun_root.join("packages/app/package.json"),
		bun_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert_eq!(
		default_lockfile_commands(&bun_package),
		vec![monochange_core::LockfileCommandExecution {
			command: "bun install --lockfile-only".to_string(),
			cwd: monochange_core::normalize_path(&bun_root.join("packages/app")),
			shell: monochange_core::ShellConfig::None,
		}]
	);
}

#[test]
fn update_json_dependency_fields_only_changes_declared_dependencies() {
	let mut manifest = json!({
		"dependencies": {
			"core": "^1.0.0",
			"left-pad": "1.3.0"
		},
		"devDependencies": {
			"core": "^1.0.0"
		}
	});
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_json_dependency_fields(
		&mut manifest,
		&["dependencies", "devDependencies"],
		&versions,
	);

	assert_eq!(
		manifest.pointer("/dependencies/core"),
		Some(&json!("2.0.0"))
	);
	assert_eq!(
		manifest.pointer("/dependencies/left-pad"),
		Some(&json!("1.3.0"))
	);
	assert_eq!(
		manifest.pointer("/devDependencies/core"),
		Some(&json!("2.0.0"))
	);
}

#[test]
fn update_package_lock_updates_root_packages_and_dependencies() {
	let mut lock = json!({
		"name": "app",
		"version": "1.0.0",
		"packages": {
			"": {
				"name": "app",
				"version": "1.0.0"
			},
			"packages/core": {
				"name": "core",
				"version": "1.0.0"
			},
			"packages/util": {
				"version": "1.0.0"
			}
		},
		"dependencies": {
			"core": {
				"version": "1.0.0"
			}
		}
	});
	let package_paths = BTreeMap::from([
		("util".to_string(), PathBuf::from("packages/util")),
		("core".to_string(), PathBuf::from("packages/core")),
	]);
	let raw_versions = BTreeMap::from([
		("app".to_string(), "2.0.0".to_string()),
		("core".to_string(), "2.1.0".to_string()),
		("util".to_string(), "3.0.0".to_string()),
	]);

	update_package_lock(&mut lock, &package_paths, &raw_versions);

	assert_eq!(lock.pointer("/version"), Some(&json!("2.0.0")));
	assert_eq!(lock.pointer("/packages//version"), Some(&json!("2.0.0")));
	assert_eq!(
		lock.pointer("/packages/packages~1core/version"),
		Some(&json!("2.1.0"))
	);
	assert_eq!(
		lock.pointer("/packages/packages~1util/version"),
		Some(&json!("3.0.0"))
	);
	assert_eq!(
		lock.pointer("/dependencies/core/version"),
		Some(&json!("2.1.0"))
	);
}

#[test]
fn update_pnpm_lock_skips_link_and_workspace_dependencies() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
importers:
  .:
    dependencies:
      core: 1.0.0
      linked: link:../linked
      workspace_dep: workspace:*
packages:
  core@1.0.0:
    dependencies:
      core: 1.0.0
snapshots:
  core@1.0.0:
    dependencies:
      core:
        version: 1.0.0
      linked:
        version: link:../linked
",
	)
	.unwrap_or_else(|error| panic!("pnpm lock yaml: {error}"));
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_pnpm_lock(&mut lock, &raw_versions);

	let rendered = serde_yaml_ng::to_string(&YamlValue::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pnpm lock: {error}"));
	assert!(rendered.contains("core: 2.0.0"));
	assert!(rendered.contains("linked: link:../linked"));
	assert!(rendered.contains("workspace_dep: workspace:*"));
}

#[test]
fn update_pnpm_lock_text_preserves_existing_formatting() {
	let lock = r#"lockfileVersion: "9.0"

settings:
  autoInstallPeers: true
  excludeLinksFromLockfile: false

importers:
  .:
    dependencies:
      core: 1.0.0
      linked: link:../linked

  npm/skill: {}
"#;
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		r#"lockfileVersion: "9.0"

settings:
  autoInstallPeers: true
  excludeLinksFromLockfile: false

importers:
  .:
    dependencies:
      core: 2.0.0
      linked: link:../linked

  npm/skill: {}
"#
	);
}

#[test]
fn update_pnpm_lock_text_returns_original_contents_when_no_entries_match() {
	let lock = "lockfileVersion: \"9.0\"\n\nimporters:\n  .: {}\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(updated, lock);
}

#[test]
fn pnpm_text_helper_functions_cover_edge_cases() {
	let contents = "importers:\n\n  # comment\n";
	let ranges = crate::yaml_line_ranges(contents);
	assert_eq!(
		crate::find_yaml_key_lines(contents, &ranges, 0, "importers"),
		vec![0]
	);
	assert!(crate::parse_yaml_line(contents, *ranges.get(1).expect("blank line range")).is_none());
	assert!(crate::parse_yaml_line(": nope", (0, 6)).is_none());
	let mut replacements = Vec::new();
	crate::collect_pnpm_section_replacements(
		contents,
		&ranges,
		1,
		&BTreeMap::new(),
		&mut replacements,
	);
	let outer_blank = "importers:\n\n  .:\n";
	let outer_blank_ranges = crate::yaml_line_ranges(outer_blank);
	let outer_blank_index =
		*crate::find_yaml_key_lines(outer_blank, &outer_blank_ranges, 0, "importers")
			.first()
			.unwrap_or_else(|| panic!("expected importers section"));
	crate::collect_pnpm_section_replacements(
		outer_blank,
		&outer_blank_ranges,
		outer_blank_index,
		&BTreeMap::new(),
		&mut replacements,
	);
	crate::collect_pnpm_dependency_replacements(
		contents,
		&ranges,
		1,
		&BTreeMap::new(),
		&mut replacements,
	);
	assert!(replacements.is_empty());
	assert!(crate::yaml_value_span("version: # comment", 0, 8).is_none());
	assert_eq!(crate::find_yaml_quote_end("\"1.0.0\"", '"'), Some(6));
	assert_eq!(crate::find_yaml_quote_end("\"1.0.0", '"'), None);
	assert_eq!(crate::render_yaml_scalar("\"1.0.0\"", "2.0.0"), "\"2.0.0\"");
	assert_eq!(crate::render_yaml_scalar("'1.0.0'", "2.0.0"), "'2.0.0'");
	assert_eq!(crate::render_yaml_scalar("1.0.0", "2.0.0"), "2.0.0");
	assert!(crate::yaml_scalar_is_updatable("1.0.0"));
	assert!(!crate::yaml_scalar_is_updatable("1"));
	assert!(!crate::yaml_scalar_is_updatable("link:../linked"));
	assert!(crate::is_pnpm_dependency_field("dependencies"));
	assert!(!crate::is_pnpm_dependency_field("resolution"));
}

#[test]
fn update_pnpm_lock_text_updates_nested_versions_and_preserves_quotes() {
	let lock = r#"lockfileVersion: '9.0'

importers:
  .:
    dependencies:
      core:
        version: "1.0.0"
      linked:
        version: link:../linked
      numeric:
        version: 1
      missing:
        path: ../missing

snapshots:
  core@1.0.0:
    optionalDependencies:
      core: '1.0.0'
"#;
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert!(updated.contains("version: \"2.0.0\""));
	assert!(updated.contains("core: '2.0.0'"));
	assert!(updated.contains("version: link:../linked"));
	assert!(updated.contains("version: 1"));
	assert!(updated.contains("path: ../missing"));
}

#[test]
fn parse_yaml_line_strips_key_quoting() {
	let contents = "'importers':\n  .:\n    dependencies:\n      'core': 1.0.0\n      \"other\": 2.0.0\n      bare: 3.0.0\n";
	let ranges = crate::yaml_line_ranges(contents);
	let keys = ranges
		.iter()
		.filter_map(|range| crate::parse_yaml_line(contents, *range))
		.map(|line| line.key.to_string())
		.collect::<Vec<_>>();
	assert_eq!(
		keys,
		vec!["importers", ".", "dependencies", "core", "other", "bare"]
	);
}

#[test]
fn unquote_yaml_key_keeps_bare_and_malformed_keys() {
	assert_eq!(crate::unquote_yaml_key("core"), "core");
	assert_eq!(crate::unquote_yaml_key("'core'"), "core");
	assert_eq!(crate::unquote_yaml_key("\"core\""), "core");
	assert_eq!(crate::unquote_yaml_key("'"), "'");
	assert_eq!(crate::unquote_yaml_key("'core"), "'core");
	assert_eq!(crate::unquote_yaml_key("'core' tail"), "'core' tail");
	assert_eq!(crate::unquote_yaml_key("'a' 'b'"), "'a' 'b'");
}

#[test]
fn update_pnpm_lock_text_updates_single_quoted_keys_and_keeps_quotes() {
	let lock =
		"lockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies:\n      'core': 1.0.0\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		"lockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies:\n      'core': 2.0.0\n"
	);
}

#[test]
fn update_pnpm_lock_text_updates_double_quoted_keys_and_keeps_quotes() {
	let lock =
		"lockfileVersion: \"9.0\"\n\nimporters:\n  .:\n    dependencies:\n      \"core\": 1.0.0\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		"lockfileVersion: \"9.0\"\n\nimporters:\n  .:\n    dependencies:\n      \"core\": 2.0.0\n"
	);
}

#[test]
fn update_pnpm_lock_text_updates_bare_keys() {
	let lock = "importers:\n  .:\n    dependencies:\n      core: 1.0.0\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		"importers:\n  .:\n    dependencies:\n      core: 2.0.0\n"
	);
}

#[test]
fn update_pnpm_lock_text_updates_quoted_scoped_keys_in_inline_and_nested_forms() {
	let lock = r"lockfileVersion: '9.0'

importers:
  .:
    dependencies:
      '@acme/api': 2.3.1

  packages/consumer:
    dependencies:
      '@acme/api':
        specifier: ^2.3.1
        version: 2.3.1
";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		r"lockfileVersion: '9.0'

importers:
  .:
    dependencies:
      '@acme/api': 2.4.0

  packages/consumer:
    dependencies:
      '@acme/api':
        specifier: ^2.3.1
        version: 2.4.0
"
	);
}

#[test]
fn update_pnpm_lock_text_skips_quoted_link_and_workspace_references() {
	let lock = r#"lockfileVersion: '9.0'

importers:
  packages/linked-consumer:
    dependencies:
      '@acme/api': 'link:../api'

  packages/workspace-consumer:
    dependencies:
      '@acme/api': "workspace:*"

  packages/nested-consumer:
    dependencies:
      '@acme/api':
        specifier: workspace:*
        version: link:../../packages/api
"#;
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(updated, lock);
}

#[test]
fn update_pnpm_lock_text_updates_keys_with_yaml_quote_escapes() {
	let lock =
		"importers:\n  .:\n    dependencies:\n      'core''s': 1.0.0\n      \"core\\\"s\": 1.0.0\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([
			("core's".to_string(), "2.0.0".to_string()),
			("core\"s".to_string(), "2.0.0".to_string()),
		]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert_eq!(
		updated,
		"importers:\n  .:\n    dependencies:\n      'core''s': 2.0.0\n      \"core\\\"s\": 2.0.0\n"
	);
}

#[test]
fn pnpm_replacement_helpers_skip_invalid_spans_and_blank_lines() {
	let mut replacements = Vec::new();
	crate::push_pnpm_scalar_replacement("link:../linked", (0, 14), "2.0.0", &mut replacements);
	crate::push_pnpm_scalar_replacement("1.0.0", (0, 99), "2.0.0", &mut replacements);
	assert!(replacements.is_empty());

	let contents = r"importers:
  .:

    dependencies:
      other: 1.0.0
      core:
        path: ../core

      next: 1.0.0
";
	let ranges = crate::yaml_line_ranges(contents);
	let section_index = *crate::find_yaml_key_lines(contents, &ranges, 0, "importers")
		.first()
		.unwrap_or_else(|| panic!("expected importers section"));
	crate::collect_pnpm_section_replacements(
		contents,
		&ranges,
		section_index,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&mut replacements,
	);
	assert!(replacements.is_empty());
}

#[test]
fn update_pnpm_lock_text_rewrites_pnpm12_two_document_lockfiles() {
	let lock = "---\nlockfileVersion: '9.0'\n\nimporters:\n  .:\n    configDependencies: {}\n    packageManagerDependencies:\n      pnpm:\n        specifier: 12.3.4\n        version: 12.3.4\n\npackages:\n  '@pnpm/logger@5.2.0':\n    resolution: {integrity: sha512-abcdef}\n\nsnapshots:\n  '@pnpm/logger@5.2.0':\n    dependencies:\n      bole: 5.0.20\n\n---\nlockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies:\n      '@acme/api': 2.3.1\n\n  packages/consumer:\n    dependencies:\n      '@acme/api':\n        specifier: ^2.3.1\n        version: 2.3.1\n";
	let updated = update_pnpm_lock_text(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update pnpm lock text: {error}"));
	assert!(updated.contains("'@acme/api': 2.4.0\n"));
	assert!(updated.contains("specifier: ^2.3.1\n        version: 2.4.0\n"));
	// The env document keeps its own package-manager pin and dependencies.
	assert!(updated.contains("specifier: 12.3.4\n        version: 12.3.4\n"));
	assert!(updated.contains("bole: 5.0.20"));
	assert!(updated.contains(
		"\n---\nlockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies:\n      '@acme/api': 2.4.0"
	));
	assert_eq!(updated.matches("---").count(), 2);
}

#[test]
fn update_pnpm_lock_text_rejects_invalid_and_empty_documents() {
	let broken_second_document = "lockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies:\n      core: 1.0.0\n\n---\nlockfileVersion: '9.0'\nimporters: [broken\n";
	let error = update_pnpm_lock_text(
		broken_second_document,
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	)
	.expect_err("broken second document should fail validation");
	let message = error.to_string();
	assert!(
		message.contains("failed to parse pnpm lock yaml"),
		"{message}"
	);

	// Empty lockfiles keep parsing as a null document and pass through, which
	// matches the previous single-document parser behavior.
	let updated = update_pnpm_lock_text("", &BTreeMap::new())
		.unwrap_or_else(|error| panic!("empty lockfile should parse: {error}"));
	assert_eq!(updated, "");
}

#[test]
fn validate_versioned_file_accepts_multi_document_pnpm_locks() {
	let directory = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let pnpm_lock = directory.path().join("pnpm-lock.yaml");
	fs::write(
		&pnpm_lock,
		"---\nlockfileVersion: '9.0'\npackages: {}\n\n---\nlockfileVersion: '9.0'\nimporters:\n  .: {}\n",
	)
	.unwrap_or_else(|error| panic!("write pnpm lock: {error}"));
	assert!(crate::validate_versioned_file(&pnpm_lock, "pnpm-lock.yaml", None).is_ok());

	fs::write(&pnpm_lock, "importers: [broken\n")
		.unwrap_or_else(|error| panic!("write pnpm lock: {error}"));
	let result = crate::validate_versioned_file(&pnpm_lock, "pnpm-lock.yaml", None);
	let message = result
		.expect_err("invalid pnpm lock should fail validation")
		.to_string();
	assert!(message.contains("is not valid pnpm lock yaml"), "{message}");

	let yarn_lock = directory.path().join("yarn.lock");
	fs::write(&yarn_lock, "# yarn lockfile v1\n")
		.unwrap_or_else(|error| panic!("write yarn lock: {error}"));
	assert!(crate::validate_versioned_file(&yarn_lock, "yarn.lock", None).is_ok());

	let missing_lock = directory.path().join("missing/pnpm-lock.yaml");
	let result = crate::validate_versioned_file(&missing_lock, "pnpm-lock.yaml", None);
	let message = result
		.expect_err("unreadable pnpm lock should fail validation")
		.to_string();
	assert!(message.contains("is not readable"), "{message}");

	let unsupported = directory.path().join("requirements.txt");
	let result = crate::validate_versioned_file(&unsupported, "requirements.txt", None);
	let message = result
		.expect_err("unsupported file should fail validation")
		.to_string();
	assert!(
		message.contains("is not supported for the npm ecosystem"),
		"{message}"
	);
}

#[test]
fn update_yarn_lock_rewrites_classic_entries() {
	let lock = r#"# THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.
# yarn lockfile v1


"@acme/api@^2.3.1":
  version "2.3.1"
  resolved "https://registry.yarnpkg.com/@acme/api/-/api-2.3.1.tgz#8f14ac6d"
  integrity sha512-b18f49e79f32b4a1c4a4068fd11d1a2d
  dependencies:
    left-pad "^1.3.0"

left-pad@^1.3.0:
  version "1.3.0"
  resolved "https://registry.yarnpkg.com/left-pad/-/left-pad-1.3.0.tgz#1fba07a2"
"#;
	let updated = update_yarn_lock(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	);
	assert!(updated.contains("version \"2.4.0\""));
	// Resolved URL, integrity, nested dependency ranges, and other entries stay.
	assert!(updated.contains("api-2.3.1.tgz#8f14ac6d"));
	assert!(updated.contains("left-pad \"^1.3.0\""));
	assert!(updated.contains("version \"1.3.0\""));
	assert!(updated.starts_with("# THIS IS AN AUTOGENERATED FILE"));
}

#[test]
fn update_yarn_lock_rewrites_berry_entries_and_preserves_workspace_placeholders() {
	let lock = r#"# This file is automatically generated by @yarnpkg/repo. Do not edit it manually.

__metadata:
  version: 8
  cacheKey: 10c0

"@acme/api@npm:2.3.1":
  version: 2.3.1
  resolution: "@acme/api@npm:2.3.1"
  checksum: 10c0/b18f49e79f32b4a1c4a4068fd11d1a2d
  languageName: node
  linkType: hard

"@acme/api@workspace:packages/api, @acme/api@^2.3.1":
  version: 0.0.0-use.local
  resolution: "@acme/api@workspace:packages/api"
  languageName: unknown
  linkType: soft
"#;
	let updated = update_yarn_lock(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	);
	assert!(updated.contains("version: 2.4.0\n"));
	assert!(updated.contains("resolution: \"@acme/api@npm:2.3.1\""));
	assert!(updated.contains("version: 0.0.0-use.local"));
	assert!(updated.contains("version: 8"));
}

#[test]
fn update_yarn_lock_returns_input_unchanged_without_matching_entries() {
	let lock = "\"lodash@^4.0.0\":\n  version \"4.17.21\"\n";
	let updated = update_yarn_lock(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	);
	assert_eq!(updated, lock);
}

#[test]
fn yarn_helpers_cover_edge_cases() {
	assert_eq!(
		crate::yarn_specifier_package_name("\"@acme/api@^1.0.0\""),
		Some("@acme/api")
	);
	assert_eq!(
		crate::yarn_specifier_package_name("left-pad@^1.3.0"),
		Some("left-pad")
	);
	assert_eq!(
		crate::yarn_specifier_package_name("abbrev@1"),
		Some("abbrev")
	);
	assert_eq!(
		crate::yarn_specifier_package_name("@acme/api"),
		Some("@acme/api")
	);
	assert_eq!(crate::yarn_specifier_package_name("\"\""), None);
	assert_eq!(crate::yarn_version_value_offset("version: 1.2.3"), Some(9));
	assert_eq!(
		crate::yarn_version_value_offset("version \"1.2.3\""),
		Some(8)
	);
	assert_eq!(crate::yarn_version_value_offset("versioned: nope"), None);
	assert_eq!(crate::yarn_version_value_offset("resolved \"...\""), None);
	assert_eq!(crate::yarn_version_value_end("\"1.2.3\""), 7);
	assert_eq!(crate::yarn_version_value_end("1.2.3  "), 5);
	assert_eq!(crate::yarn_version_value_end("\"unterminated"), 13);
	assert!(crate::parse_yarn_entry_header("# comment only", (0, 14)).is_none());
	assert!(crate::parse_yarn_entry_header("  nested: value", (0, 15)).is_none());
	assert!(crate::parse_yarn_entry_header("not-a-key", (0, 9)).is_none());
	assert_eq!(
		crate::parse_yarn_entry_header("\"@acme/api@npm:2.3.1\":", (0, 22))
			.map(|header| (header.package_name, header.workspace_resolved)),
		Some(("@acme/api", false))
	);
	assert_eq!(
		crate::parse_yarn_entry_header("\"@acme/api@workspace:packages/api\":", (0, 35))
			.map(|header| (header.package_name, header.workspace_resolved)),
		Some(("@acme/api", true))
	);
	// The raw collector rewrites any version line; the workspace skip happens
	// in `update_yarn_lock` before the collector runs.
	let workspace_entry = "\"@acme/api@workspace:packages/api\":\n  version: 0.0.0-use.local\n";
	let ranges = crate::yaml_line_ranges(workspace_entry);
	let mut replacements = Vec::new();
	crate::collect_yarn_entry_version_replacement(
		workspace_entry,
		&ranges,
		1,
		"9.9.9",
		&mut replacements,
	);
	assert_eq!(replacements.len(), 1);
	assert_eq!(replacements[0].1, "9.9.9");
}

#[test]
fn update_bun_lock_rewrites_matching_versions() {
	let updated = update_bun_lock(
		"{\n  \"core\": \"1.0.0\",\n  \"other\": \"0.1.0\"\n}",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert!(updated.contains("\"core\": \"2.0.0\""));
	assert!(updated.contains("\"other\": \"0.1.0\""));
}

#[test]
fn update_bun_lock_rewrites_jsonc_pins_and_descriptors() {
	let lock = "{\n  \"lockfileVersion\": 1,\n  \"workspaces\": {\n    \"\": {\n      \"name\": \"fixture\",\n      \"dependencies\": {\n        \"@acme/api\": \"workspace:packages/api\",\n      },\n    },\n    \"packages/consumer\": {\n      \"name\": \"@acme/consumer\",\n      \"dependencies\": {\n        \"@acme/api\": \"2.3.1\",\n      },\n    },\n  },\n  \"packages\": {\n    \"@acme/api\": [\"@acme/api@2.3.1\", \"\", {\n      \"os\": [\"darwin\"],\n    }, \"sha512-abcdef\"],\n    \"@acme/consumer\": [\"@acme/consumer@workspace:packages/consumer\"],\n    \"left-pad\": [\"left-pad@1.3.0\", \"\", \"sha512-1fba07a2\"],\n  },\n  \"overrides\": {},\n}\n";
	let updated = update_bun_lock(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	);
	assert!(updated.contains("\"@acme/api\": \"2.4.0\""));
	assert!(updated.contains("[\"@acme/api@2.4.0\", \"\""));
	// Workspace links, external pins, and JSONC trailing commas survive.
	assert!(updated.contains("\"@acme/api\": \"workspace:packages/api\""));
	assert!(updated.contains("\"left-pad\": [\"left-pad@1.3.0\""));
	assert!(updated.contains("\"overrides\": {},"));
	assert_eq!(updated.matches("2.3.1").count(), 0);
}

#[test]
fn update_bun_lock_leaves_ranges_protocols_and_aliases_alone() {
	let lock = "{\n  \"workspaces\": {\n    \"\": {\n      \"dependencies\": {\n        \"@acme/api\": \"^2.3.1\",\n        \"aliased\": \"npm:@acme/api@2.3.1\",\n        \"vendored\": \"@acme/api@github:acme/api#deadbeef\",\n      },\n    },\n  },\n  \"packages\": {\n    \"@acme/api\": [\"@acme/api@workspace:packages/api\"],\n  },\n}\n";
	let updated = update_bun_lock(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	);
	assert_eq!(updated, lock);
}

#[test]
fn update_yarn_lock_skips_malformed_and_unchanged_version_lines() {
	// Comments, non-version properties, and empty version values inside an
	// entry are skipped, and an already-matching version stays untouched.
	let lock = "\"@acme/api@^1.0.0\":\n  # regenerated by hand\n  resolution: \"@acme/api@npm:1.0.0\"\n\"@acme/api@^2.3.1\":\n  version:\n  version: 2.4.0\n";
	let updated = update_yarn_lock(
		lock,
		&BTreeMap::from([("@acme/api".to_string(), "2.4.0".to_string())]),
	);
	assert_eq!(updated, lock);
	assert!(updated.contains("version: 2.4.0"));
}

#[test]
fn update_bun_lock_skips_truncated_and_mismatched_descriptors() {
	// Truncated entries, unterminated descriptors, and descriptors that
	// belong to a different package leave the input untouched.
	let truncated = update_bun_lock(
		"{\"core\": [\"",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert_eq!(truncated, "{\"core\": [\"");

	let unterminated = update_bun_lock(
		"{\"core\": [\"core@1",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert_eq!(unterminated, "{\"core\": [\"core@1");

	let mismatched = update_bun_lock(
		"{\"core\": [\"left-pad@1.3.0\"]}",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert_eq!(mismatched, "{\"core\": [\"left-pad@1.3.0\"]}");
}

#[test]
fn update_bun_lock_binary_rewrites_all_occurrences() {
	let updated = update_bun_lock_binary(
		b"core@1.0.0\0core@1.0.0\0",
		&BTreeMap::from([("core".to_string(), "1.0.0".to_string())]),
		&BTreeMap::from([("core".to_string(), "2.1.0".to_string())]),
	);
	let rendered = String::from_utf8(updated).unwrap_or_else(|error| panic!("utf8: {error}"));
	assert_eq!(rendered.matches("2.1.0").count(), 2);
}

#[test]
fn adapter_discover_matches_direct_npm_discovery() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace");
	let from_adapter = adapter()
		.discover(&fixture_root)
		.unwrap_or_else(|error| panic!("adapter discovery: {error}"));
	let direct = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("direct discovery: {error}"));
	assert_eq!(from_adapter.packages, direct.packages);
	assert_eq!(from_adapter.warnings, direct.warnings);
}

#[test]
fn discovers_object_style_package_json_workspaces_and_warnings() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/workspace-object-patterns");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.packages.len(), 3);
	assert!(discovery.warnings.iter().any(|warning| {
		warning.contains("missing/*") && warning.contains("matched no packages")
	}));
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "root-workspace")
	);
	let private_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "object-private")
		.unwrap_or_else(|| panic!("expected object-private package"));
	assert_eq!(private_package.publish_state, PublishState::Private);
	let web_package = discovery
		.packages
		.iter()
		.find(|package| package.name == "object-web")
		.unwrap_or_else(|| panic!("expected object-web package"));
	assert_eq!(
		web_package.metadata.get("manager").map(String::as_str),
		Some("npm")
	);
}

#[test]
fn discover_standalone_package_defaults_manager_to_npm() {
	let fixture_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/standalone-package");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.warnings, Vec::<String>::new());
	assert_eq!(discovery.packages.len(), 1);
	let package = discovery
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected standalone package"));
	assert_eq!(package.name, "standalone-app");
	assert_eq!(
		package.metadata.get("manager").map(String::as_str),
		Some("npm")
	);
}

#[test]
fn discover_multiple_standalone_packages_keep_unique_manifest_ids() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/standalone-multiple-packages");
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.warnings, Vec::<String>::new());
	assert_eq!(discovery.packages.len(), 2);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.id == "npm:packages/docs/package.json")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.id == "npm:packages/web/package.json")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "standalone-docs")
	);
	assert!(
		discovery
			.packages
			.iter()
			.any(|package| package.name == "standalone-web")
	);
}

#[test]
fn load_configured_npm_package_normalizes_ids_relative_to_root() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/standalone-multiple-packages");
	let package = load_configured_npm_package(&fixture_root, &fixture_root.join("packages/docs"))
		.unwrap_or_else(|error| panic!("configured npm package: {error}"))
		.unwrap_or_else(|| panic!("expected configured npm package"));
	assert_eq!(package.id, "npm:packages/docs/package.json");
	assert_eq!(package.name, "standalone-docs");
}

#[test]
fn normalize_package_id_leaves_existing_id_when_manifest_is_outside_root() {
	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		"standalone-docs",
		PathBuf::from("/tmp/outside-root/package.json"),
		PathBuf::from("/tmp/outside-root"),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let original_id = package.id.clone();
	super::normalize_package_id(Path::new("/tmp/workspace-root"), &mut package);
	assert_eq!(package.id, original_id);
}

#[test]
fn update_json_dependency_fields_ignores_missing_or_non_object_sections() {
	let mut manifest = json!({
		"dependencies": "not-an-object",
		"scripts": {
			"build": "vite build"
		}
	});
	let versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_json_dependency_fields(
		&mut manifest,
		&["dependencies", "devDependencies"],
		&versions,
	);

	assert_eq!(manifest.get("dependencies"), Some(&json!("not-an-object")));
	assert_eq!(
		manifest.get("scripts"),
		Some(&json!({"build": "vite build"}))
	);
}

#[test]
fn workspace_pattern_helpers_cover_array_object_and_missing_cases() {
	assert_eq!(
		workspace_patterns_from_package_json(&json!({"workspaces": ["packages/*"]})),
		vec!["packages/*".to_string()]
	);
	assert_eq!(
		workspace_patterns_from_package_json(&json!({"workspaces": {"packages": ["apps/*"]}})),
		vec!["apps/*".to_string()]
	);
	assert_eq!(
		workspace_patterns_from_package_json(&json!({})),
		Vec::<String>::new()
	);
}

#[test]
fn detect_npm_manager_prefers_bun_then_pnpm_then_npm() {
	let bun_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-bun");
	assert_eq!(detect_npm_manager(&bun_root), "bun");
	let yarn_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-yarn");
	assert_eq!(detect_npm_manager(&yarn_root), "yarn");
	let pnpm_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/npm/workspace-pnpm");
	assert_eq!(detect_npm_manager(&pnpm_root), "pnpm");
	let npm_root =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/npm/standalone-package");
	assert_eq!(detect_npm_manager(&npm_root), "npm");
}

#[test]
fn explicit_file_workspace_patterns_discover_package_manifests() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/workspace-explicit-file");
	let mut warnings = Vec::new();
	let manifests = expand_member_patterns(
		&fixture_root,
		&["packages/web/package.json".to_string()],
		&mut warnings,
	);
	assert_eq!(warnings, Vec::<String>::new());
	assert_eq!(manifests.len(), 1);
	assert!(
		manifests
			.iter()
			.any(|path| path.ends_with("packages/web/package.json"))
	);

	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(
		discovery
			.packages
			.first()
			.unwrap_or_else(|| panic!("expected explicit package"))
			.name,
		"explicit-web"
	);
}

#[test]
fn package_json_parsing_and_workspace_detection_report_parse_errors() {
	let invalid_workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join(
		"../../fixtures/tests/npm/invalid-workspace-package-json/invalid-workspace-package.json",
	);
	let error = package_json_declares_workspaces(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected invalid workspace parse error"));
	assert!(error.to_string().contains("failed to parse"));

	let workspace_error = discover_package_json_workspace(&invalid_workspace)
		.err()
		.unwrap_or_else(|| panic!("expected workspace discovery error"));
	assert!(workspace_error.to_string().contains("failed to parse"));

	let invalid_pnpm = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/invalid-pnpm-workspace/invalid-pnpm-workspace.yaml");
	let pnpm_error = discover_pnpm_workspace(&invalid_pnpm)
		.err()
		.unwrap_or_else(|| panic!("expected pnpm parse error"));
	assert!(pnpm_error.to_string().contains("failed to parse"));

	let invalid_package = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/invalid-package-json/invalid-package.json");
	let package_error = parse_package_json(&invalid_package, Path::new("."), "npm")
		.err()
		.unwrap_or_else(|| panic!("expected package parse error"));
	assert!(package_error.to_string().contains("failed to parse"));
}

#[test]
fn update_package_lock_ignores_unmapped_root_and_non_object_entries() {
	let mut lock = json!({
		"name": "app",
		"version": "1.0.0",
		"packages": {
			"": {"name": "app", "version": "1.0.0"},
			"packages/core": "not-an-object",
			"packages/util": {"version": "1.0.0"}
		},
		"dependencies": {
			"core": "1.0.0",
			"util": {"version": "1.0.0"}
		}
	});
	let package_paths = BTreeMap::from([("util".to_string(), PathBuf::from("packages/util"))]);
	let raw_versions = BTreeMap::from([("util".to_string(), "2.0.0".to_string())]);

	update_package_lock(&mut lock, &package_paths, &raw_versions);

	assert_eq!(lock.pointer("/version"), Some(&json!("1.0.0")));
	assert_eq!(lock.pointer("/packages//version"), Some(&json!("1.0.0")));
	assert_eq!(
		lock.pointer("/packages/packages~1core"),
		Some(&json!("not-an-object"))
	);
	assert_eq!(
		lock.pointer("/packages/packages~1util/version"),
		Some(&json!("2.0.0"))
	);
	assert_eq!(lock.pointer("/dependencies/core"), Some(&json!("1.0.0")));
	assert_eq!(
		lock.pointer("/dependencies/util/version"),
		Some(&json!("2.0.0"))
	);
}

#[test]
fn update_pnpm_lock_covers_missing_sections_and_non_string_versions() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
importers:
  .:
    devDependencies:
      core: 1.0.0
packages:
  ignored: plain-text
snapshots:
  core@1.0.0:
    peerDependencies:
      core:
        version: 1
      linked:
        version: link:../linked
",
	)
	.unwrap_or_else(|error| panic!("pnpm lock yaml: {error}"));
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_pnpm_lock(&mut lock, &raw_versions);

	let rendered = serde_yaml_ng::to_string(&YamlValue::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pnpm lock: {error}"));
	assert!(rendered.contains("core: 2.0.0"));
	assert!(rendered.contains("version: 1"));
	assert!(rendered.contains("link:../linked"));
}

#[test]
fn update_pnpm_lock_updates_nested_non_workspace_version_mappings() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
snapshots:
  core@1.0.0:
    dependencies:
      core:
        version: 1.0.0
",
	)
	.unwrap_or_else(|error| panic!("pnpm lock yaml: {error}"));
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_pnpm_lock(&mut lock, &raw_versions);

	let rendered = serde_yaml_ng::to_string(&YamlValue::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pnpm lock: {error}"));
	assert!(rendered.contains("version: 2.0.0"));
}

#[test]
fn update_pnpm_lock_skips_workspace_references_inside_nested_version_mappings() {
	let mut lock: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(
		r"
snapshots:
  core@1.0.0:
    dependencies:
      core:
        version: workspace:*
",
	)
	.unwrap_or_else(|error| panic!("pnpm lock yaml: {error}"));
	let raw_versions = BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);

	update_pnpm_lock(&mut lock, &raw_versions);

	let rendered = serde_yaml_ng::to_string(&YamlValue::Mapping(lock))
		.unwrap_or_else(|error| panic!("render pnpm lock: {error}"));
	assert!(rendered.contains("version: workspace:*"));
}

#[test]
fn update_bun_lock_and_binary_skip_unusable_replacements() {
	let unchanged = update_bun_lock(
		"{\n  \"core\": \"1.0.0\n}",
		&BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
	);
	assert_eq!(unchanged, "{\n  \"core\": \"1.0.0\n}");

	let binary = update_bun_lock_binary(
		b"core@1.0.0\0same@2.0.0\0empty@\0",
		&BTreeMap::from([
			("missing".to_string(), "1.0.0".to_string()),
			("same".to_string(), "2.0.0".to_string()),
			("empty".to_string(), String::new()),
		]),
		&BTreeMap::from([
			("same".to_string(), "2.0.0".to_string()),
			("empty".to_string(), "3.0.0".to_string()),
		]),
	);
	assert_eq!(binary, b"core@1.0.0\0same@2.0.0\0empty@\0");
}

#[test]
fn default_dependency_version_prefix_is_correct() {
	assert_eq!(super::default_dependency_version_prefix(), "^");
}

#[test]
fn default_dependency_fields_are_non_empty() {
	assert!(!super::default_dependency_fields().is_empty());
}

#[test]
fn validate_versioned_file_accepts_valid_package_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, r#"{"version": "1.0.0"}"#).unwrap_or_else(|error| panic!("write: {error}"));
	assert!(super::validate_versioned_file(&path, "package.json", None).is_ok());
}

#[test]
fn validate_versioned_file_accepts_custom_field() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, r#"{"customVersion": "1.0.0"}"#)
		.unwrap_or_else(|error| panic!("write: {error}"));
	let custom_fields = vec!["customVersion".to_string()];
	assert!(super::validate_versioned_file(&path, "package.json", Some(&custom_fields)).is_ok());
}

#[test]
fn validate_versioned_file_rejects_invalid_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, "not json").unwrap_or_else(|error| panic!("write: {error}"));
	let result = super::validate_versioned_file(&path, "package.json", None);
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not valid JSON"));
}

#[test]
fn validate_versioned_file_rejects_missing_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, r#"{"name": "test"}"#).unwrap_or_else(|error| panic!("write: {error}"));
	let result = super::validate_versioned_file(&path, "package.json", None);
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("does not contain a `version` string field")
	);
}

#[test]
fn validate_versioned_file_rejects_missing_file() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("missing.json");
	let result = super::validate_versioned_file(&path, "missing.json", None);
	assert!(result.is_err());
	assert!(result.unwrap_err().to_string().contains("not readable"));
}

#[test]
fn validate_versioned_file_accepts_dotted_custom_field() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(
		&path,
		r#"{"name": "test", "metadata": {"bin": {"monochange": {"version": "1.0.0"}}}}"#,
	)
	.unwrap_or_else(|error| panic!("write: {error}"));
	let custom_fields = vec!["metadata.bin.monochange.version".to_string()];
	assert!(super::validate_versioned_file(&path, "package.json", Some(&custom_fields)).is_ok());
}

#[test]
fn validate_versioned_file_rejects_missing_dotted_custom_field() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, r#"{"name": "test", "metadata": {"bin": {}}}"#)
		.unwrap_or_else(|error| panic!("write: {error}"));
	let custom_fields = vec!["metadata.bin.monochange.version".to_string()];
	let result = super::validate_versioned_file(&path, "package.json", Some(&custom_fields));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("does not contain a `metadata.bin.monochange.version` string field")
	);
}

#[test]
fn validate_versioned_file_validates_every_custom_field() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, r#"{"version": "1.0.0"}"#).unwrap_or_else(|error| panic!("write: {error}"));
	let custom_fields = vec!["version".to_string(), "missing".to_string()];
	let result = super::validate_versioned_file(&path, "package.json", Some(&custom_fields));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("does not contain a `missing` string field")
	);
}

#[test]
fn validate_versioned_file_treats_empty_custom_fields_as_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("package.json");
	fs::write(&path, r#"{"name": "test"}"#).unwrap_or_else(|error| panic!("write: {error}"));
	// An explicitly empty field list falls back to the `version` default, so a
	// manifest without `version` is still rejected.
	let result = super::validate_versioned_file(&path, "package.json", Some(&[]));
	assert!(result.is_err());
	assert!(
		result
			.unwrap_err()
			.to_string()
			.contains("does not contain a `version` string field")
	);
}

#[test]
fn workspace_pattern_skips_directories_without_package_json() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/workspace-missing-package-json");

	// Discovery should succeed and only find packages with package.json files,
	// skipping directories that match the glob but don't contain a package.json.
	let discovery = discover_npm_packages(&fixture_root)
		.unwrap_or_else(|error| panic!("npm discovery should succeed when workspace pattern matches directories without package.json: {error}"));

	// Only the directory with a package.json should be discovered
	let names: Vec<_> = discovery.packages.iter().map(|p| p.name.clone()).collect();
	assert!(
		names.contains(&"has-package".to_string()),
		"expected 'has-package' in {names:?}"
	);
	assert!(
		!names.contains(&"no-package-json".to_string()),
		"'no-package-json' should not be discovered since it has no package.json, got {names:?}"
	);
}

#[test]
fn npm_trust_command_wraps_npm_with_pnpm_for_pnpm_managed_packages() {
	let request = PublishRequest {
		package_id: "pkg".to_string(),
		package_name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from("package.json"),
		package_root: PathBuf::from("."),
		registry: RegistryKind::Npm,
		package_manager: Some("pnpm".to_string()),
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		flow: monochange_core::PublishFlow::Direct,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		timeout: PublishTimeoutSettings::default(),
		fail_on_duplicate: false,
		placeholder_readme: "placeholder".to_string(),
	};
	let context = GitHubTrustContext {
		repository: "owner/repo".to_string(),
		workflow: "release.yml".to_string(),
		environment: None,
	};

	let command = crate::build_npm_trust_command(&request, &context);

	assert_eq!(command.program, "pnpm");
	assert_eq!(
		command.args.iter().map(String::as_str).collect::<Vec<_>>(),
		vec![
			"exec",
			"npm",
			"trust",
			"github",
			"pkg",
			"--file",
			"release.yml",
			"--repo",
			"owner/repo",
			"--yes"
		]
	);
	assert!(command.env.is_empty());
}
