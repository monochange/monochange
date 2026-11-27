//! Tests for the sync versions feature in `monochange_dart`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use monochange_core::VersionStrategy;

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
