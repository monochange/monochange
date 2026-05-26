//! Tests for the sync versions feature in `monochange_dart`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use monochange_core::VersionStrategy;

// --- sync_internal_dependency_versions tests ---

#[test]
fn sync_dart_detects_string_internal_deps() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  my_package: ^0.5.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].dependency_name, "my_package");
	assert_eq!(changes[0].old_value, "^0.5.0");
	assert_eq!(changes[0].new_value, "^0.7.0");
	assert_eq!(changes[0].section, "dependencies");
}

#[test]
fn sync_dart_skips_external_deps() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  http: ^1.0.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert!(changes.is_empty(), "no changes expected for external deps");
}

#[test]
fn sync_dart_skips_already_matching_deps() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  my_package: ^0.7.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert!(
		changes.is_empty(),
		"no changes expected when already matching"
	);
}

#[test]
fn sync_dart_converts_path_dep_under_workspace_resolution() {
	let pubspec = r"
name: my_app
version: 1.0.0
resolution: workspace
dependencies:
  my_package:
    path: ../my_package
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "2.1.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].dependency_name, "my_package");
	assert_eq!(changes[0].new_value, "^2.1.0");
}

#[test]
fn sync_dart_exact_strategy() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  my_package: ^0.5.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Exact,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].new_value, "0.7.0");
}

#[test]
fn sync_dart_compatible_strategy() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  my_package: ^0.5.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Compatible,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].new_value, ">=0.7.0");
}

#[test]
fn sync_dart_dev_dependencies_and_dependency_overrides() {
	let pubspec = r"
name: my_app
version: 1.0.0
dev_dependencies:
  my_package: ^0.5.0
dependency_overrides:
  my_package: ^0.4.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert_eq!(changes.len(), 2);
	let sections: Vec<&str> = changes.iter().map(|c| c.section.as_str()).collect();
	assert!(sections.contains(&"dev_dependencies"));
	assert!(sections.contains(&"dependency_overrides"));
}

#[test]
fn sync_dart_mapping_dep_with_version_under_no_workspace_resolution() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  my_package:
    version: ^0.5.0
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].new_value, "^0.7.0");
}

#[test]
fn sync_dart_invalid_yaml_returns_error() {
	let pubspec = "{{invalid yaml";
	let version_map: BTreeMap<String, String> = BTreeMap::new();
	let workspace_names: BTreeSet<String> = BTreeSet::new();
	let result = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	);
	assert!(result.is_err());
}

#[test]
fn sync_dart_reports_mapping_detail_scalar_values() {
	let pubspec = r"
name: my_app
version: 1.0.0
resolution: workspace
dependencies:
  my_package:
    path: ../my_package
    hosted: true
    optional: null
    priority: 1
    tags:
      - local
";
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my_package".to_string(), "2.1.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));

	assert_eq!(changes.len(), 1);
	assert!(changes[0].old_value.contains("hosted: true"));
	assert!(changes[0].old_value.contains("optional: null"));
	assert!(changes[0].old_value.contains("priority: 1"));
	assert!(changes[0].old_value.contains("tags: [...]"));
}

#[test]
fn sync_dart_skips_non_string_keys_missing_versions_and_mapping_without_version() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  1: ^1.0.0
  missing_package: ^1.0.0
  mapped_package:
    path: ../mapped_package
";
	let version_map: BTreeMap<String, String> = BTreeMap::new();
	let workspace_names: BTreeSet<String> =
		BTreeSet::from(["missing_package".to_string(), "mapped_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));

	assert!(changes.is_empty());
}

#[test]
fn sync_dart_skips_internal_deps_with_non_string_scalar_values() {
	let pubspec = r"
name: my_app
version: 1.0.0
dependencies:
  numeric_package: 1
  bool_package: true
";
	let version_map: BTreeMap<String, String> = BTreeMap::from([
		("numeric_package".to_string(), "2.0.0".to_string()),
		("bool_package".to_string(), "3.0.0".to_string()),
	]);
	let workspace_names: BTreeSet<String> =
		BTreeSet::from(["numeric_package".to_string(), "bool_package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		pubspec,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.unwrap_or_else(|error| panic!("sync: {error}"));

	assert!(changes.is_empty());
}
