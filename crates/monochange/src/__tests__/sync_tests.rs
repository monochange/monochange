//! Tests for sync module types and functions.

use std::ffi::OsString;

use clap::Command;
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
	let result = sync::SyncResult { changes: vec![] };
	let output = sync::format_sync_result(&result, false, true);
	assert!(
		output.is_empty(),
		"empty changes should produce empty output when quiet"
	);
}

#[test]
fn format_sync_result_reports_update() {
	let result = sync::SyncResult {
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
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
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
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
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
	};
	let output = sync::format_sync_result(&result, false, true);
	assert!(
		!output.contains("dry run"),
		"non-dry-run should not have dry run footer"
	);
}

#[test]
fn format_sync_result_empty_not_quiet_still_returns_empty() {
	let result = sync::SyncResult { changes: vec![] };
	let output = sync::format_sync_result(&result, false, false);
	assert!(
		output.is_empty(),
		"empty changes should return empty string even when not quiet"
	);
}

#[test]
fn format_sync_result_not_quiet_and_not_dry_run_eprints_output() {
	let result = sync::SyncResult {
		changes: vec![sync::FileSyncResult {
			path: "pubspec.yaml".to_string(),
			changes: vec![DependencySyncChange {
				dependency_name: "my_package".to_string(),
				section: "dependencies".to_string(),
				old_value: "^0.5.0".to_string(),
				new_value: "^0.7.0".to_string(),
			}],
		}],
	};
	let output = sync::format_sync_result(&result, false, false);
	assert!(
		output.contains("updated ^0.5.0 → ^0.7.0 in my_package (pubspec.yaml)"),
		"expected update message in non-quiet output"
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
		changes: vec![dep_change.clone()],
	};
	let result = sync::SyncResult {
		changes: vec![file_result],
	};
	assert_eq!(result.changes.len(), 1);
	assert_eq!(result.changes[0].path, "pubspec.yaml");
	assert_eq!(result.changes[0].changes.len(), 1);
	assert_eq!(result.changes[0].changes[0].dependency_name, "my_package");
}

// --- build_sync_subcommand tests ---

#[test]
fn build_sync_subcommand_parses_versions_with_defaults() {
	let command = Command::new("mc").subcommand(cli::build_sync_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("sync"),
			OsString::from("versions"),
		])
		.unwrap_or_else(|error| panic!("sync matches: {error}"));
	let (_, sync_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected sync subcommand"));
	let (sub_name, versions_matches) = sync_matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert_eq!(sub_name, "versions");
	assert!(!versions_matches.get_flag("dry-run"));
	assert_eq!(
		versions_matches
			.get_one::<String>("strategy")
			.map(String::as_str),
		Some("default"),
		"strategy should default to 'default'"
	);
}

#[test]
fn build_sync_subcommand_parses_versions_with_dry_run() {
	let command = Command::new("mc").subcommand(cli::build_sync_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("sync"),
			OsString::from("versions"),
			OsString::from("--dry-run"),
		])
		.unwrap_or_else(|error| panic!("sync matches: {error}"));
	let (_, sync_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected sync subcommand"));
	let (_, versions_matches) = sync_matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert!(versions_matches.get_flag("dry-run"));
}

#[test]
fn build_sync_subcommand_parses_versions_with_strategy() {
	let command = Command::new("mc").subcommand(cli::build_sync_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("sync"),
			OsString::from("versions"),
			OsString::from("--strategy"),
			OsString::from("exact"),
		])
		.unwrap_or_else(|error| panic!("sync matches: {error}"));
	let (_, sync_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected sync subcommand"));
	let (_, versions_matches) = sync_matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected versions subcommand"));
	assert_eq!(
		versions_matches
			.get_one::<String>("strategy")
			.map(String::as_str),
		Some("exact")
	);
}
