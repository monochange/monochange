//! Tests for sync module types and functions.

use std::ffi::OsString;

use clap::Command;
use monochange_core::CliCommandDefinition;
use monochange_core::DependencySyncChange;
use monochange_core::Ecosystem;
use monochange_core::VersionStrategy;

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

// --- build_versions_subcommand tests ---

#[test]
fn build_versions_subcommand_parses_defaults() {
	let command = Command::new("mc").subcommand(cli::build_versions_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([OsString::from("mc"), OsString::from("versions")])
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
	let command = Command::new("mc").subcommand(cli::build_versions_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
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
	let command = Command::new("mc").subcommand(cli::build_versions_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
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
fn build_cli_skips_config_defined_versions_command() {
	let cli_command = CliCommandDefinition {
		name: "versions".to_string(),
		help_text: Some("legacy read-only versions command".to_string()),
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};
	let command = cli::build_command_with_cli("mc", &[cli_command]);
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
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
		"mc",
		[
			OsString::from("mc"),
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
