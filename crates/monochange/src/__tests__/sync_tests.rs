//! Tests for sync module types and functions.

use std::ffi::OsString;

use clap::Command;
use monochange_core::CliCommandDefinition;
use monochange_core::DependencySyncChange;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::VersionStrategy;
use semver::Version;

use crate::cli;
use crate::sync;

// --- apply_sync_changes tests ---

#[test]
fn apply_sync_changes_dart_manifest_updates_version() {
	let contents = r"name: my_app
version: 1.0.0
dependencies:
  my_package: ^0.5.0
";
	let changes = vec![DependencySyncChange {
		dependency_name: "my_package".to_string(),
		section: "dependencies".to_string(),
		old_value: "^0.5.0".to_string(),
		new_value: "^0.7.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Dart)
		.unwrap_or_else(|error| panic!("apply_sync_changes: {error}"));
	assert!(result.contains("^0.7.0"), "expected ^0.7.0 in result");
	assert!(
		!result.contains("^0.5.0"),
		"expected ^0.5.0 absent from result"
	);
}

#[test]
fn apply_sync_changes_npm_manifest_updates_version() {
	let contents = r#"{"name":"my-app","version":"1.0.0","dependencies":{"my-package":"^0.5.0"}}"#;
	let changes = vec![DependencySyncChange {
		dependency_name: "my-package".to_string(),
		section: "dependencies".to_string(),
		old_value: "^0.5.0".to_string(),
		new_value: "^0.7.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Npm)
		.unwrap_or_else(|error| panic!("apply_sync_changes: {error}"));
	assert!(result.contains("^0.7.0"), "expected ^0.7.0 in result");
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
fn apply_sync_changes_go_manifest_updates_version() {
	let contents = "module app\n\nrequire example.com/core v1.0.0\n";
	let changes = vec![DependencySyncChange {
		dependency_name: "example.com/core".to_string(),
		section: "require".to_string(),
		old_value: "v1.0.0".to_string(),
		new_value: "v2.0.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Go)
		.unwrap_or_else(|error| panic!("apply go changes: {error}"));
	assert!(result.contains("example.com/core v2.0.0"));
}

#[test]
fn apply_sync_changes_deno_manifest_updates_import() {
	let contents = r#"{"imports":{"core":"^1.0.0"}}"#;
	let changes = vec![DependencySyncChange {
		dependency_name: "core".to_string(),
		section: "imports".to_string(),
		old_value: "^1.0.0".to_string(),
		new_value: "^2.0.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Deno)
		.unwrap_or_else(|error| panic!("apply deno changes: {error}"));
	assert!(result.contains("^2.0.0"));
}

#[test]
fn detect_supported_ecosystem_changes() {
	let versions = std::collections::BTreeMap::from([("core".to_string(), "2.0.0".to_string())]);
	let names = std::collections::BTreeSet::from(["core".to_string()]);

	let cargo = sync::detect_cargo_changes(
		"[package]\nname = \"app\"\nversion = \"1.0.0\"\n\n[dependencies]\ncore = \"1.0.0\"\n",
		&versions,
		&names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect cargo: {error}"));
	assert_eq!(cargo[0].new_value, "2.0.0");

	let python = sync::detect_python_changes(
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\ndependencies = [\"core>=1.0.0\"]\n",
		&versions,
		&names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect python: {error}"));
	assert_eq!(python[0].new_value, ">=2.0.0");

	let go = sync::detect_go_changes(
		"module app\n\nrequire core v1.0.0\n",
		&versions,
		&names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect go: {error}"));
	assert_eq!(go[0].new_value, "v2.0.0");

	let deno = sync::detect_deno_changes(
		r#"{"imports":{"core":"^1.0.0"}}"#,
		&versions,
		&names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect deno: {error}"));
	assert_eq!(deno[0].new_value, "^2.0.0");
}

#[test]
fn apply_sync_changes_unsupported_ecosystem_returns_contents_unchanged() {
	let contents = "[package]\nname = \"my-crate\"\nversion = \"1.0.0\"\n";
	let changes = vec![DependencySyncChange {
		dependency_name: "my-crate".to_string(),
		section: "dependencies".to_string(),
		old_value: "1.0.0".to_string(),
		new_value: "1.1.0".to_string(),
	}];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Cargo)
		.unwrap_or_else(|error| panic!("apply_sync_changes: {error}"));
	assert_eq!(
		result, contents,
		"unsupported ecosystem should return contents unchanged"
	);
}

#[test]
fn apply_sync_changes_dart_with_empty_changes_preserves_content() {
	let contents = r"name: my_app
version: 1.0.0
";
	let result = sync::apply_sync_changes(contents, &[], Ecosystem::Dart)
		.unwrap_or_else(|error| panic!("apply_sync_changes: {error}"));
	assert!(
		result.contains("name: my_app"),
		"empty changes should preserve original content"
	);
}

#[test]
fn apply_sync_changes_dart_multiple_deps() {
	let contents = r"name: my_app
version: 1.0.0
dependencies:
  pkg_a: ^0.1.0
  pkg_b: ^2.0.0
";
	let changes = vec![
		DependencySyncChange {
			dependency_name: "pkg_a".to_string(),
			section: "dependencies".to_string(),
			old_value: "^0.1.0".to_string(),
			new_value: "^0.2.0".to_string(),
		},
		DependencySyncChange {
			dependency_name: "pkg_b".to_string(),
			section: "dependencies".to_string(),
			old_value: "^2.0.0".to_string(),
			new_value: "^3.0.0".to_string(),
		},
	];
	let result = sync::apply_sync_changes(contents, &changes, Ecosystem::Dart)
		.unwrap_or_else(|error| panic!("apply_sync_changes: {error}"));
	assert!(result.contains("^0.2.0"), "expected pkg_a updated");
	assert!(result.contains("^3.0.0"), "expected pkg_b updated");
}

// --- format_sync_result tests ---

#[test]
fn format_sync_result_empty_changes_returns_empty_string() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![],
		skipped: vec![],
	};
	let output = sync::format_sync_result(&result, false, true);
	assert!(
		output.is_empty(),
		"empty changes should produce empty output when quiet"
	);
}

#[test]
fn format_sync_result_reports_update() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			ecosystem: Ecosystem::Dart,
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
		skipped: vec![],
	};
	let output = sync::format_sync_result(&result, false, true);
	assert!(
		output.contains("updated ^0.5.0 → ^0.7.0 in my_package (pubspec.yaml)"),
		"expected update message in output"
	);
}

#[test]
fn format_sync_result_dry_run_prefixes_with_would() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			ecosystem: Ecosystem::Dart,
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
		skipped: vec![],
	};
	let output = sync::format_sync_result(&result, true, true);
	assert!(
		output.contains("would update ^0.5.0 → ^0.7.0"),
		"expected 'would update' prefix in dry-run output"
	);
	assert!(
		output.contains("dry run — no files were modified"),
		"expected dry-run footer"
	);
}

#[test]
fn format_sync_result_non_dry_run_has_no_dry_run_footer() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			ecosystem: Ecosystem::Dart,
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
		skipped: vec![],
	};
	let output = sync::format_sync_result(&result, false, true);
	assert!(
		!output.contains("dry run"),
		"non-dry-run should not have dry run footer"
	);
}

#[test]
fn format_sync_result_with_changes_and_skips_separates_sections() {
	let dep_change = DependencySyncChange {
		dependency_name: "my_package".to_string(),
		section: "dependencies".to_string(),
		old_value: "^0.5.0".to_string(),
		new_value: "^0.7.0".to_string(),
	};
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			ecosystem: Ecosystem::Dart,
			changes: vec![dep_change],
		}],
		skipped: vec![sync::SkippedSyncPackage {
			path: "go.mod".to_string(),
			package_name: "go-lib".to_string(),
			ecosystem: Ecosystem::Go,
			reason: "ecosystem sync is not implemented yet".to_string(),
		}],
	};

	let output = sync::format_sync_result(&result, false, true);
	assert!(output.contains("updated ^0.5.0 → ^0.7.0"));
	assert!(output.contains("\nSkipped unsupported ecosystems:"));
	assert!(output.contains("- go.mod (Go): ecosystem sync is not implemented yet"));
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

	let text =
		sync::format_sync_result_for_cli(&result, true, true, sync::VersionsOutputFormat::Text);
	let json =
		sync::format_sync_result_for_cli(&result, true, true, sync::VersionsOutputFormat::Json);
	assert!(text.contains("Skipped unsupported ecosystems:"));
	assert!(json.contains(r#""package_name": "go-lib""#));
}

#[test]
fn parse_versions_output_format_defaults_to_text() {
	assert_eq!(
		sync::parse_versions_output_format("json"),
		sync::VersionsOutputFormat::Json
	);
	assert_eq!(
		sync::parse_versions_output_format("text"),
		sync::VersionsOutputFormat::Text
	);
	assert_eq!(
		sync::parse_versions_output_format("unknown"),
		sync::VersionsOutputFormat::Text
	);
}

#[test]
fn format_sync_result_empty_not_quiet_still_returns_empty() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![],
		skipped: vec![],
	};
	let output = sync::format_sync_result(&result, false, false);
	assert!(
		output.is_empty(),
		"empty changes should return empty string even when not quiet"
	);
}

#[test]
fn format_sync_result_not_quiet_and_not_dry_run_returns_output() {
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			ecosystem: Ecosystem::Dart,
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
		skipped: vec![],
	};
	let output = sync::format_sync_result(&result, false, false);
	assert!(
		output.contains("updated ^0.5.0 → ^0.7.0 in my_package (pubspec.yaml)"),
		"expected update message in output"
	);
}

// --- parse_strategy tests ---

#[test]
fn parse_strategy_exact() {
	assert_eq!(sync::parse_strategy("exact"), VersionStrategy::Exact);
}

#[test]
fn parse_strategy_caret() {
	assert_eq!(sync::parse_strategy("caret"), VersionStrategy::Caret);
}

#[test]
fn parse_strategy_compatible() {
	assert_eq!(
		sync::parse_strategy("compatible"),
		VersionStrategy::Compatible
	);
}

#[test]
fn parse_strategy_default_fallback() {
	assert_eq!(sync::parse_strategy("default"), VersionStrategy::Default);
	assert_eq!(sync::parse_strategy("unknown"), VersionStrategy::Default);
	assert_eq!(sync::parse_strategy(""), VersionStrategy::Default);
}

// --- struct construction tests ---

#[test]
fn sync_result_tracks_file_changes() {
	let dep_change = DependencySyncChange {
		dependency_name: "my_package".to_string(),
		section: "dependencies".to_string(),
		old_value: "^0.5.0".to_string(),
		new_value: "^0.7.0".to_string(),
	};
	let file_result = sync::FileSyncResult {
		path: "pubspec.yaml".to_string(),
		ecosystem: Ecosystem::Dart,
		changes: vec![dep_change.clone()],
	};
	let result = sync::SyncResult {
		applied: false,
		strategy: VersionStrategy::Default,
		changes: vec![file_result],
		skipped: vec![],
	};
	assert_eq!(result.changes.len(), 1);
	assert_eq!(result.changes[0].path, "pubspec.yaml");
	assert_eq!(result.changes[0].changes.len(), 1);
	assert_eq!(result.changes[0].changes[0].dependency_name, "my_package");
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
fn plan_discovered_workspace_versions_wraps_detect_errors() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = temp.path();
	let app_manifest = root.join("packages/app/package.json");
	let lib_manifest = root.join("packages/lib/package.json");
	std::fs::create_dir_all(
		app_manifest
			.parent()
			.unwrap_or_else(|| panic!("app parent")),
	)
	.unwrap_or_else(|error| panic!("create app dir: {error}"));
	std::fs::create_dir_all(
		lib_manifest
			.parent()
			.unwrap_or_else(|| panic!("lib parent")),
	)
	.unwrap_or_else(|error| panic!("create lib dir: {error}"));
	std::fs::write(&app_manifest, "{")
		.unwrap_or_else(|error| panic!("write invalid package manifest: {error}"));
	std::fs::write(&lib_manifest, r#"{"name":"lib","version":"1.2.3"}"#)
		.unwrap_or_else(|error| panic!("write lib manifest: {error}"));

	let app = PackageRecord::new(
		Ecosystem::Npm,
		"app",
		app_manifest,
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lib = PackageRecord::new(
		Ecosystem::Npm,
		"lib",
		lib_manifest,
		root.to_path_buf(),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);
	let discovery = DiscoveryReport {
		workspace_root: root.to_path_buf(),
		packages: vec![app, lib],
		dependencies: Vec::new(),
		version_groups: Vec::new(),
		warnings: Vec::new(),
	};

	let error =
		sync::plan_discovered_workspace_versions(root, VersionStrategy::Default, &discovery)
			.unwrap_err();
	let message = error.to_string();
	assert!(message.contains("failed to detect Npm version sync changes"));
	assert!(message.contains("package.json"));
}

#[test]
fn apply_version_sync_plan_wraps_apply_errors() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest_path = temp.path().join("package.json");
	let plan = sync::VersionSyncPlan {
		strategy: VersionStrategy::Default,
		files: vec![sync::VersionSyncPlanFile {
			path: "package.json".to_string(),
			ecosystem: Ecosystem::Npm,
			changes: vec![DependencySyncChange {
				dependency_name: "lib".to_string(),
				section: "dependencies".to_string(),
				old_value: "^1.0.0".to_string(),
				new_value: "^1.2.3".to_string(),
			}],
			manifest_path,
			contents: "{".to_string(),
		}],
		skipped: Vec::new(),
	};

	let error = sync::apply_version_sync_plan(&plan, true).unwrap_err();
	let message = error.to_string();
	assert!(message.contains("failed to apply Npm version sync changes"));
	assert!(message.contains("package.json"));
}

// --- build_versions_subcommand tests ---

#[test]
fn build_versions_subcommand_parses_defaults() {
	let command = Command::new("monochange").subcommand(cli::build_versions_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([OsString::from("monochange"), OsString::from("versions")])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));
	let (_, versions_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert!(!versions_matches.get_flag("dry-run"));
	assert_eq!(
		versions_matches
			.get_one::<String>("strategy")
			.map(String::as_str),
		Some("default")
	);
	assert_eq!(
		versions_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("text")
	);
}

#[test]
fn build_versions_subcommand_parses_dry_run_and_json() {
	let command = Command::new("monochange").subcommand(cli::build_versions_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));
	let (_, versions_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert!(versions_matches.get_flag("dry-run"));
	assert_eq!(
		versions_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("json")
	);
}

#[test]
fn build_versions_subcommand_parses_strategy() {
	let command = Command::new("monochange").subcommand(cli::build_versions_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("--strategy"),
			OsString::from("exact"),
		])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));
	let (_, versions_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert_eq!(
		versions_matches
			.get_one::<String>("strategy")
			.map(String::as_str),
		Some("exact")
	);
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
fn build_versions_subcommand_long_help_describes_examples() {
	let mut command = cli::build_command_with_cli("monochange", &[]);
	let versions = command
		.find_subcommand_mut("versions")
		.unwrap_or_else(|| panic!("versions subcommand should exist"));
	let help = versions
		.get_after_help()
		.unwrap_or_else(|| panic!("versions after help should exist"))
		.to_string();
	assert!(help.contains("monochange versions --dry-run"));
	assert!(help.contains("monochange versions --dry-run --format json"));
	assert!(help.contains("This command syncs internal workspace dependency constraints."));
	assert!(help.contains("Strategy precedence is package config"));
}

#[test]
fn sync_context_error_includes_operation_ecosystem_path_and_source() {
	let source = crate::MonochangeError::Config("invalid manifest".to_string());
	let error = sync::sync_context_error(
		"detect",
		Ecosystem::Npm,
		std::path::Path::new("packages/app/package.json"),
		&source,
	);
	let message = error.to_string();
	assert!(message.contains("failed to detect Npm version sync changes"));
	assert!(message.contains("packages/app/package.json"));
	assert!(message.contains("invalid manifest"));
}

#[test]
fn strategy_name_reports_all_versions_strategies() {
	assert_eq!(sync::strategy_name(VersionStrategy::Default), "default");
	assert_eq!(sync::strategy_name(VersionStrategy::Exact), "exact");
	assert_eq!(sync::strategy_name(VersionStrategy::Caret), "caret");
	assert_eq!(
		sync::strategy_name(VersionStrategy::Compatible),
		"compatible"
	);
}

#[test]
fn build_cli_skips_config_defined_versions_command() {
	let cli_command = CliCommandDefinition {
		name: "versions".to_string(),
		help_text: Some("legacy read-only versions command".to_string()),
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};
	let command = cli::build_command_with_cli("monochange", &[cli_command]);
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("--dry-run"),
		])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));
	assert_eq!(matches.subcommand().map(|(name, _)| name), Some("versions"));
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

	crate::cli_runtime::block_on_in_context(Box::pin(crate::run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("versions"),
			OsString::from("--dry-run"),
			OsString::from("--strategy"),
			OsString::from("exact"),
		],
		root,
	)))
	.unwrap_or_else(|error| panic!("versions: {error}"));
}

#[test]
fn sync_workspace_versions_reads_npm_manifests_from_fixture() {
	let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.join("fixtures/npm/workspace");
	let result = sync::sync_workspace_versions(&root, VersionStrategy::Exact, true)
		.unwrap_or_else(|error| panic!("sync workspace versions: {error}"));

	assert_eq!(result.changes.len(), 1);
	assert!(
		result.changes[0]
			.path
			.ends_with("packages/web/package.json")
	);
	assert_eq!(result.changes[0].changes[0].new_value, "1.0.0");
}

#[test]
fn sync_workspace_versions_writes_npm_manifest_in_temp_workspace() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = temp.path();
	std::fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
parent_bump = "patch"
package_type = "npm"

[package.shared]
path = "packages/shared"

[package.web]
path = "packages/web"

[ecosystems.npm]
enabled = true
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::fs::create_dir_all(root.join("packages/shared"))
		.unwrap_or_else(|error| panic!("mkdir shared: {error}"));
	std::fs::create_dir_all(root.join("packages/web"))
		.unwrap_or_else(|error| panic!("mkdir web: {error}"));
	std::fs::write(
		root.join("packages/shared/package.json"),
		r#"{"name":"npm-shared","version":"1.0.0"}"#,
	)
	.unwrap_or_else(|error| panic!("write shared package: {error}"));
	std::fs::write(
		root.join("packages/web/package.json"),
		r#"{"name":"npm-web","version":"1.0.0","dependencies":{"npm-shared":"^0.9.0"}}"#,
	)
	.unwrap_or_else(|error| panic!("write web package: {error}"));

	let result = sync::sync_workspace_versions(root, VersionStrategy::Exact, false)
		.unwrap_or_else(|error| panic!("sync workspace versions: {error}"));
	let updated = std::fs::read_to_string(root.join("packages/web/package.json"))
		.unwrap_or_else(|error| panic!("read web package: {error}"));

	assert_eq!(result.changes.len(), 1);
	assert!(updated.contains(r#""npm-shared":"1.0.0""#));
}

#[test]
fn sync_workspace_versions_returns_empty_when_packages_have_no_versions() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = temp.path();
	std::fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
parent_bump = "patch"
package_type = "npm"

[package.shared]
path = "packages/shared"

[ecosystems.npm]
enabled = true
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::fs::create_dir_all(root.join("packages/shared"))
		.unwrap_or_else(|error| panic!("mkdir shared: {error}"));
	std::fs::write(
		root.join("packages/shared/package.json"),
		r#"{"name":"npm-shared"}"#,
	)
	.unwrap_or_else(|error| panic!("write shared package: {error}"));

	let result = sync::sync_workspace_versions(root, VersionStrategy::Default, true)
		.unwrap_or_else(|error| panic!("sync workspace versions: {error}"));

	assert!(result.changes.is_empty());
}

#[test]
fn read_manifest_reports_missing_file() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = temp.path().join("missing.json");
	let result = sync::read_manifest(&missing);
	let error = match result {
		Ok(contents) => panic!("expected missing file error, got: {contents}"),
		Err(error) => error,
	};

	assert!(error.to_string().contains("failed to read"));
}

#[test]
fn write_manifest_reports_missing_parent() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing_parent = temp.path().join("missing/package.json");
	let result = sync::write_manifest(&missing_parent, "{}".to_string());
	let error = match result {
		Ok(()) => panic!("expected missing parent write error"),
		Err(error) => error,
	};

	assert!(error.to_string().contains("failed to write"));
}

#[test]
fn detect_python_changes_with_empty_dep_name_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("mypkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("mypkg".to_string());
	// Empty string in dependencies array should be skipped
	let contents = "[project]\ndependencies = [\"\"]\n";
	let result = sync::detect_python_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_python_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_go_changes_with_non_workspace_require_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("example.com/pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("example.com/pkg".to_string());
	// require with a package not in workspace_package_names
	let contents = "module test\n\nrequire other.com/pkg v1.0.0\n";
	let result = sync::detect_go_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_go_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_deno_changes_with_non_string_import_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("mypkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("mypkg".to_string());
	// Import value is not a string
	let contents = r#"{"imports": {"mypkg": 123}}"#;
	let result = sync::detect_deno_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_deno_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn apply_sync_changes_with_go_ecosystem_returns_contents() {
	// Go doesn't have a sync adapter, so it should return contents unchanged
	let contents = "module test\n";
	let result = sync::apply_sync_changes(contents, &[], monochange_core::Ecosystem::Go)
		.unwrap_or_else(|error| panic!("apply_sync_changes: {error}"));
	assert_eq!(result, contents);
}

#[test]
fn detect_python_changes_with_invalid_toml_returns_error() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let result = sync::detect_python_changes(
		"not valid toml [[[[",
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	);
	assert!(result.is_err());
}

#[test]
fn detect_python_changes_with_non_matching_dep_name_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("other-pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("other-pkg".to_string());
	let contents = "[project]\ndependencies = [\"mypkg>=1.0\"]\n";
	let result = sync::detect_python_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_python_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_python_changes_with_no_version_in_map_skips() {
	let version_map = std::collections::BTreeMap::new();
	let mut names = std::collections::BTreeSet::new();
	names.insert("mypkg".to_string());
	let contents = "[project]\ndependencies = [\"mypkg>=1.0\"]\n";
	let result = sync::detect_python_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_python_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_go_changes_with_no_matching_require_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("example.com/pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("example.com/pkg".to_string());
	let contents = "module test\n";
	let result = sync::detect_go_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_go_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_deno_changes_with_non_matching_import_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("other-pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("other-pkg".to_string());
	let contents = r#"{"imports": {"mypkg": "npm:mypkg@1.0.0"}}"#;
	let result = sync::detect_deno_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_deno_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn normalize_python_package_name_normalizes_separators() {
	assert_eq!(sync::normalize_python_package_name("foo-bar"), "foo-bar");
	assert_eq!(sync::normalize_python_package_name("foo_bar"), "foo-bar");
	assert_eq!(sync::normalize_python_package_name("foo.bar"), "foo-bar");
	assert_eq!(sync::normalize_python_package_name("FOO-BAR"), "foo-bar");
	assert_eq!(sync::normalize_python_package_name("foo--bar"), "foo-bar");
}

#[test]
fn extract_python_version_constraint_parses_version() {
	assert_eq!(
		sync::extract_python_version_constraint("mypackage>=1.0", "mypackage"),
		Some(">=1.0".to_string()),
	);
	assert_eq!(
		sync::extract_python_version_constraint("mypackage", "mypackage"),
		None,
	);
	assert_eq!(
		sync::extract_python_version_constraint("mypackage[extra]>=1.0", "mypackage"),
		Some(">=1.0".to_string()),
	);
}

#[test]
fn version_prefix_for_strategy_exact_returns_empty() {
	assert_eq!(
		sync::version_prefix_for_strategy(
			monochange_core::Ecosystem::Npm,
			monochange_core::VersionStrategy::Exact
		),
		""
	);
}

#[test]
fn version_prefix_for_strategy_cargo_compatible_returns_gte() {
	assert_eq!(
		sync::version_prefix_for_strategy(
			monochange_core::Ecosystem::Cargo,
			monochange_core::VersionStrategy::Compatible
		),
		">="
	);
}

#[test]
fn version_prefix_for_strategy_go_compatible_returns_gte() {
	assert_eq!(
		sync::version_prefix_for_strategy(
			monochange_core::Ecosystem::Go,
			monochange_core::VersionStrategy::Compatible
		),
		">="
	);
}

#[test]
fn version_prefix_for_strategy_cargo_default_returns_empty() {
	assert_eq!(
		sync::version_prefix_for_strategy(
			monochange_core::Ecosystem::Cargo,
			monochange_core::VersionStrategy::Default
		),
		""
	);
}

#[test]
fn version_prefix_for_strategy_go_default_returns_empty() {
	assert_eq!(
		sync::version_prefix_for_strategy(
			monochange_core::Ecosystem::Go,
			monochange_core::VersionStrategy::Default
		),
		""
	);
}

#[test]
fn target_constraint_go_prepends_v() {
	let result = sync::target_constraint(
		monochange_core::Ecosystem::Go,
		"1.0.0",
		monochange_core::VersionStrategy::Default,
	);
	assert!(
		result.starts_with('v'),
		"Go versions should start with v, got: {result}"
	);
}

#[test]
fn target_constraint_go_with_v_prefix_keeps_it() {
	let result = sync::target_constraint(
		monochange_core::Ecosystem::Go,
		"v1.0.0",
		monochange_core::VersionStrategy::Default,
	);
	assert!(
		result.starts_with("v1"),
		"Go versions with v prefix should keep it, got: {result}"
	);
}

#[test]
fn target_constraint_npm_exact_returns_version() {
	let result = sync::target_constraint(
		monochange_core::Ecosystem::Npm,
		"1.2.3",
		monochange_core::VersionStrategy::Exact,
	);
	assert_eq!(result, "1.2.3");
}

#[test]
fn target_constraint_npm_compatible_returns_caret() {
	let result = sync::target_constraint(
		monochange_core::Ecosystem::Npm,
		"1.2.3",
		monochange_core::VersionStrategy::Compatible,
	);
	assert!(
		result.starts_with('^'),
		"Compatible for npm should start with ^, got: {result}"
	);
}

#[test]
fn target_constraint_npm_caret_returns_caret() {
	let result = sync::target_constraint(
		monochange_core::Ecosystem::Npm,
		"1.2.3",
		monochange_core::VersionStrategy::Caret,
	);
	assert!(
		result.starts_with('^'),
		"Caret should start with ^, got: {result}"
	);
}

#[test]
fn detect_cargo_changes_with_invalid_toml_returns_error() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let result = sync::detect_cargo_changes(
		"not valid toml [[[[",
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	);
	assert!(result.is_err());
}

#[test]
fn detect_cargo_changes_with_missing_table_returns_empty() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let contents = "[package]\nname = 'test'";
	let result = sync::detect_cargo_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_cargo_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_cargo_changes_with_missing_dep_returns_empty() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("other-pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("other-pkg".to_string());
	let contents = "[dependencies]\npkg = '1.0.0'";
	let result = sync::detect_cargo_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_cargo_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_cargo_changes_with_missing_version_in_map_returns_empty() {
	let version_map = std::collections::BTreeMap::new();
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let contents = "[dependencies]\npkg = '1.0.0'";
	let result = sync::detect_cargo_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_cargo_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_cargo_changes_with_inline_table_version() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "2.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let contents = "[dependencies]\npkg = { version = '1.0.0', path = '../pkg' }";
	let result = sync::detect_cargo_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_cargo_changes: {error}"));
	assert_eq!(result.len(), 1);
	assert_eq!(result[0].dependency_name, "pkg");
}

#[test]
fn detect_python_changes_with_missing_project_section_returns_empty() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let contents = "[tool]\nruff = true";
	let result = sync::detect_python_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_python_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_python_changes_with_missing_dependencies_returns_empty() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let contents = "[project]\nname = 'test'";
	let result = sync::detect_python_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_python_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_go_changes_with_package_not_in_workspace_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("example.com/pkg".to_string(), "1.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("example.com/pkg".to_string());
	let contents = "module test\n\nrequire other.com/pkg v1.0.0\n";
	let result = sync::detect_go_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_go_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_deno_changes_with_invalid_json_returns_error() {
	let result = sync::detect_deno_changes(
		"not json",
		&std::collections::BTreeMap::new(),
		&std::collections::BTreeSet::new(),
		monochange_core::VersionStrategy::Default,
	);
	assert!(result.is_err());
}

#[test]
fn detect_deno_changes_with_version_not_in_map_skips() {
	let version_map = std::collections::BTreeMap::new();
	let mut names = std::collections::BTreeSet::new();
	names.insert("mypkg".to_string());
	let contents = r#"{"imports": {"mypkg": "npm:mypkg@1.0.0"}}"#;
	let result = sync::detect_deno_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_deno_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_cargo_changes_with_inline_table_without_version_skips() {
	let mut version_map = std::collections::BTreeMap::new();
	version_map.insert("pkg".to_string(), "2.0.0".to_string());
	let mut names = std::collections::BTreeSet::new();
	names.insert("pkg".to_string());
	let contents = "[dependencies]\npkg = { path = '../pkg' }";
	let result = sync::detect_cargo_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_cargo_changes: {error}"));
	assert!(result.is_empty());
}

#[test]
fn detect_go_changes_with_version_not_in_map_skips() {
	let version_map = std::collections::BTreeMap::new();
	let mut names = std::collections::BTreeSet::new();
	names.insert("example.com/pkg".to_string());
	let contents = "module test\n\nrequire example.com/pkg v1.0.0\n";
	let result = sync::detect_go_changes(
		contents,
		&version_map,
		&names,
		monochange_core::VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("detect_go_changes: {error}"));
	assert!(result.is_empty());
}
