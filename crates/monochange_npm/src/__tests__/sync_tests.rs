//! Tests for the sync versions feature in monochange_npm.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use monochange_core::VersionStrategy;

#[test]
fn sync_npm_detects_internal_deps() {
	let package_json =
		r#"{"name":"my-app","version":"1.0.0","dependencies":{"my-package":"^0.5.0"}}"#;
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my-package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my-package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.expect("sync should succeed");
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].dependency_name, "my-package");
	assert_eq!(changes[0].old_value, "^0.5.0");
	assert_eq!(changes[0].new_value, "^0.7.0");
	assert_eq!(changes[0].section, "dependencies");
}

#[test]
fn sync_npm_skips_external_deps() {
	let package_json = r#"{"name":"my-app","version":"1.0.0","dependencies":{"express":"^4.0.0"}}"#;
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my-package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my-package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.expect("sync should succeed");
	assert!(changes.is_empty(), "no changes for external deps");
}

#[test]
fn sync_npm_skips_workspace_protocol_refs() {
	let package_json =
		r#"{"name":"my-app","version":"1.0.0","dependencies":{"my-package":"workspace:*"}}"#;
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my-package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my-package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.expect("sync should succeed");
	assert!(changes.is_empty(), "workspace:* protocol should be skipped");
}

#[test]
fn sync_npm_skips_already_matching_deps() {
	let package_json =
		r#"{"name":"my-app","version":"1.0.0","dependencies":{"my-package":"^0.7.0"}}"#;
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my-package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my-package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.expect("sync should succeed");
	assert!(changes.is_empty(), "no changes when already matching");
}

#[test]
fn sync_npm_exact_strategy() {
	let package_json =
		r#"{"name":"my-app","version":"1.0.0","dependencies":{"my-package":"^0.5.0"}}"#;
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my-package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my-package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Exact,
	)
	.expect("sync should succeed");
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].new_value, "0.7.0");
}

#[test]
fn sync_npm_scans_dev_and_peer_dependencies() {
	let package_json =
		r#"{"name":"my-app","version":"1.0.0","devDependencies":{"my-package":"^0.5.0"}}"#;
	let version_map: BTreeMap<String, String> =
		BTreeMap::from([("my-package".to_string(), "0.7.0".to_string())]);
	let workspace_names: BTreeSet<String> = BTreeSet::from(["my-package".to_string()]);
	let changes = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	)
	.expect("sync should succeed");
	assert_eq!(changes.len(), 1);
	assert_eq!(changes[0].section, "devDependencies");
}

#[test]
fn sync_npm_invalid_json_returns_error() {
	let package_json = "{{invalid json";
	let version_map: BTreeMap<String, String> = BTreeMap::new();
	let workspace_names: BTreeSet<String> = BTreeSet::new();
	let result = super::sync_internal_dependency_versions(
		package_json,
		&version_map,
		&workspace_names,
		VersionStrategy::Default,
	);
	assert!(result.is_err());
}
