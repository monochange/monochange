use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishState;
use monochange_core::PublishTimeoutSettings;
use monochange_core::RegistryKind;
use monochange_core::TrustedPublishingSettings;
use monochange_github::GitHubTrustContext;
use monochange_publish::PublishRequest;
use semver::Version;
use serde_json::json;
use serde_yaml_ng::Value as YamlValue;
use tempfile::tempdir;

use crate::default_lockfile_commands;
use crate::discover_npm_packages;
use crate::discover_package_json_workspace;
use crate::discover_pnpm_workspace;
use crate::package_json_declares_workspaces;
use crate::parse_package_json;
use crate::update_bun_lock;
use crate::update_json_dependency_fields;
use crate::update_package_lock;
use crate::update_pnpm_lock;
use crate::update_pnpm_lock_text;

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
