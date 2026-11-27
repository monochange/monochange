//! Tests for sync module types and functions.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::Path;

use clap::Command;
use monochange_core::CliCommandDefinition;
use monochange_core::DependencySyncChange;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::VersionGroup;
use monochange_core::VersionStrategy;
use semver::Version;

use crate::OutputFormat;
use crate::cli;
use crate::sync;

fn package_record(
	id: &str,
	name: &str,
	current_version: Option<Version>,
	version_group_id: Option<&str>,
) -> PackageRecord {
	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		name,
		std::path::PathBuf::from(format!("packages/{name}/package.json")),
		std::path::PathBuf::from("."),
		current_version,
		PublishState::Public,
	);
	package.id = id.to_string();
	package.version_group_id = version_group_id.map(String::from);
	package
}

#[test]
fn version_inventory_uses_configured_package_ids_and_group_ids() {
	let mut core = package_record(
		"cargo:crates/core/Cargo.toml",
		"workflow-core",
		Some(Version::new(1, 2, 3)),
		Some("sdk"),
	);
	core.metadata
		.insert("config_id".to_string(), "core".to_string());
	let mut web = package_record(
		"cargo:crates/web/Cargo.toml",
		"workflow-web",
		Some(Version::new(1, 4, 0)),
		Some("sdk"),
	);
	web.metadata
		.insert("config_id".to_string(), "web".to_string());
	let mut unversioned = package_record(
		"cargo:crates/unversioned/Cargo.toml",
		"workflow-unversioned",
		None,
		Some("empty"),
	);
	unversioned
		.metadata
		.insert("config_id".to_string(), "unversioned".to_string());
	let auto_discovered = package_record(
		"auto-package-id",
		"native-package-name",
		Some(Version::new(0, 1, 0)),
		None,
	);

	let discovery = DiscoveryReport {
		workspace_root: std::path::PathBuf::from("."),
		packages: vec![core, web, unversioned, auto_discovered],
		dependencies: Vec::new(),
		version_groups: vec![
			VersionGroup {
				group_id: "sdk".to_string(),
				display_name: "SDK".to_string(),
				members: vec![
					"cargo:crates/core/Cargo.toml".to_string(),
					"cargo:crates/web/Cargo.toml".to_string(),
				],
				member_max_bumps: std::collections::BTreeMap::new(),
				mismatch_detected: true,
			},
			VersionGroup {
				group_id: "empty".to_string(),
				display_name: "Empty".to_string(),
				members: vec!["cargo:crates/unversioned/Cargo.toml".to_string()],
				member_max_bumps: std::collections::BTreeMap::new(),
				mismatch_detected: false,
			},
		],
		warnings: Vec::new(),
	};

	let inventory = sync::version_inventory(&discovery);

	assert_eq!(inventory.get("core").map(String::as_str), Some("1.2.3"));
	assert_eq!(inventory.get("web").map(String::as_str), Some("1.4.0"));
	assert_eq!(inventory.get("sdk").map(String::as_str), Some("1.4.0"));
	assert_eq!(
		inventory.get("auto-package-id").map(String::as_str),
		Some("0.1.0")
	);
	assert!(!inventory.contains_key("cargo:crates/core/Cargo.toml"));
	assert!(!inventory.contains_key("workflow-core"));
	assert!(!inventory.contains_key("native-package-name"));
	assert!(!inventory.contains_key("unversioned"));

	assert!(!inventory.contains_key("empty"));
}

#[test]
fn apply_sync_changes_cargo_manifest_updates_version() {
	let contents =
		"[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore = \"1.0.0\"\n";
	let changes = vec![DependencySyncChange {
		dependency_name: "core".to_string(),
		section: "dependencies".to_string(),
		old_value: "1.0.0".to_string(),
		new_value: "2.0.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Cargo)
		.unwrap_or_else(|error| panic!("apply cargo changes: {error}"));
	assert!(result.contains("core = \"2.0.0\""));
}

#[test]
fn apply_sync_changes_python_manifest_updates_version() {
	let contents =
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\ndependencies = [\"core>=1.0.0\"]\n";
	let changes = vec![DependencySyncChange {
		dependency_name: "core".to_string(),
		section: "dependencies".to_string(),
		old_value: ">=1.0.0".to_string(),
		new_value: ">=2.0.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Python)
		.unwrap_or_else(|error| panic!("apply python changes: {error}"));
	assert!(result.contains("core>=2.0.0"));
}

#[test]
fn detect_supported_ecosystem_changes() {
	let versions = std::collections::BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);
	let names = std::collections::BTreeSet::from(["core".to_string()]);

	let cargo = sync::detect_cargo_changes(
		"[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore = \"1.0.0\"\n",
		&versions,
		&names,
		&BTreeMap::new(),
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect cargo: {error}"));
	assert_eq!(cargo[0].new_value, "2.0.0");

	let python = sync::detect_python_changes(
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\ndependencies = [\"core>=1.0.0\"]\n",
		&versions,
		&names,
		&BTreeMap::new(),
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect python: {error}"));
	assert_eq!(python[0].new_value, ">=2.0.0");

	let go = sync::detect_go_changes(
		"module app\n\nrequire core v1.0.0\n",
		&versions,
		&names,
		&BTreeMap::new(),
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect go: {error}"));
	assert_eq!(go[0].new_value, "v2.0.0");

	let deno = sync::detect_deno_changes(
		r#"{"imports":{"core":"^1.0.0"}}"#,
		&versions,
		&names,
		&BTreeMap::new(),
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect deno: {error}"));
	assert_eq!(deno[0].new_value, "^2.0.0");
}

#[test]
fn format_sync_result_for_cli_supports_text_and_json() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![],
		skipped: vec![sync::SkippedSyncPackage {
			path: "go.mod".to_string(),
			package_name: "go-lib".to_string(),
			ecosystem: Ecosystem::Go,
			reason: "ecosystem sync is not implemented yet".to_string(),
		}],
	};

	let root = Path::new(".");
	let text = sync::format_sync_result_for_cli(root, &result, true, true, OutputFormat::Text);
	let json = sync::format_sync_result_for_cli(root, &result, true, true, OutputFormat::Json);
	let json_min =
		sync::format_sync_result_for_cli(root, &result, true, true, OutputFormat::JsonMin);
	assert!(text.contains("Skipped unsupported ecosystems (1)"));
	assert!(json.contains(r#""package_name": "go-lib""#));
	assert!(json_min.contains(r#""package_name":"go-lib""#));

	assert!(json_min.contains('\n'));
}

#[test]
fn plan_discovered_workspace_versions_handles_supported_go_packages_without_skips() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = temp.path();
	std::fs::write(root.join("go.mod"), "module go-lib\n")
		.unwrap_or_else(|error| panic!("write go.mod: {error}"));
	let package = PackageRecord::new(
		Ecosystem::Go,
		"go-lib",
		root.join("go.mod"),
		root.to_path_buf(),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);
	let discovery = DiscoveryReport {
		workspace_root: root.to_path_buf(),
		packages: vec![package],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};

	let plan = sync::plan_discovered_workspace_versions(root, VersionStrategy::Default, &discovery)
		.unwrap_or_else(|error| panic!("plan discovered workspace versions: {error}"));
	assert!(plan.files.is_empty());
	assert!(plan.skipped.is_empty());
}

#[test]
fn cli_command_after_help_describes_configured_versions_command() {
	let cli_command = CliCommandDefinition {
		name: "versions".to_string(),
		help_text: Some("sync versions".to_string()),
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};
	let help = cli::cli_command_after_help(&cli_command)
		.unwrap_or_else(|| panic!("versions command should have after-help"));
	assert!(help.contains("monochange versions --dry-run"));
	assert!(help.contains("This command syncs internal workspace dependency constraints."));
	assert!(help.contains("Strategy precedence is package config"));
}

#[test]
fn run_with_args_dispatches_versions() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = temp.path();
	std::fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
parent_bump = "patch"
package_type = "dart"

[package.core]
path = "packages/core"

[package.app]
path = "packages/app"

[ecosystems.dart]
enabled = true
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::fs::create_dir_all(root.join("packages/core"))
		.unwrap_or_else(|error| panic!("mkdir core: {error}"));
	std::fs::create_dir_all(root.join("packages/app"))
		.unwrap_or_else(|error| panic!("mkdir app: {error}"));
	std::fs::write(
		root.join("packages/core/pubspec.yaml"),
		"name: core\nversion: 1.2.3\n",
	)
	.unwrap_or_else(|error| panic!("write core pubspec: {error}"));
	std::fs::write(
		root.join("packages/app/pubspec.yaml"),
		"name: app\nversion: 1.0.0\ndependencies:\n  core: ^1.0.0\n",
	)
	.unwrap_or_else(|error| panic!("write app pubspec: {error}"));

	let sync_output = crate::tests::block_on_in_context(Box::pin(crate::run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("sync"),
			OsString::from("--dry-run"),
			OsString::from("--strategy"),
			OsString::from("exact"),
		],
		root,
	)))
	.unwrap_or_else(|error| panic!("versions sync: {error}"));
	assert!(sync_output.contains("dry-run, no files were modified"));
	let check_output = crate::tests::block_on_in_context(Box::pin(crate::run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("--strategy"),
			OsString::from("exact"),
		],
		root,
	)))
	.unwrap_or_else(|error| panic!("versions check: {error}"));
	assert!(check_output.contains("Run `monochange versions sync` to write these changes."));
	assert_eq!(
		std::fs::read_to_string(root.join("packages/app/pubspec.yaml"))
			.unwrap_or_else(|error| panic!("read app pubspec: {error}")),
		"name: app\nversion: 1.0.0\ndependencies:\n  core: ^1.0.0\n",
		"bare `monochange versions` must not write files"
	);

	let list_output = crate::tests::block_on_in_context(Box::pin(crate::run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("list"),
			OsString::from("--format"),
			OsString::from("json"),
		],
		root,
	)))
	.unwrap_or_else(|error| panic!("versions list: {error}"));
	let inventory = sync::list_workspace_versions(root)
		.unwrap_or_else(|error| panic!("list workspace versions: {error}"));
	let expected_output = sync::format_version_inventory_for_cli(&inventory, OutputFormat::Json)
		.unwrap_or_else(|error| panic!("render inventory json: {error}"));
	assert_eq!(list_output, expected_output);
}

#[test]
fn native_ecosystems_sync_is_a_noop_without_prefixes() {
	use monochange_core::Ecosystem;
	use monochange_core::VersionStrategy;

	for ecosystem in [Ecosystem::Swift, Ecosystem::CocoaPods, Ecosystem::Gradle] {
		assert_eq!(
			sync::version_prefix_for_strategy(ecosystem, VersionStrategy::Caret),
			""
		);
	}

	let changes = sync::detect_native_changes(
		"",
		&std::collections::BTreeMap::new(),
		&std::collections::BTreeSet::new(),
		&std::collections::BTreeMap::new(),
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect native changes: {error}"));
	assert!(changes.is_empty());

	assert_eq!(
		sync::apply_native_changes("contents", &[])
			.unwrap_or_else(|error| panic!("apply native changes: {error}")),
		"contents"
	);
}
