//! Tests for sync module types.

use std::collections::BTreeMap;

use monochange_core::DependencySyncChange;

use crate::sync;

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

#[test]
fn file_sync_result_tracks_path_and_changes() {
	let dep_change = DependencySyncChange {
		dependency_name: "my_package".to_string(),
		section: "dependencies".to_string(),
		old_value: "^0.5.0".to_string(),
		new_value: "^0.7.0".to_string(),
	};
	let file_result = sync::FileSyncResult {
		path: "pubspec.yaml".to_string(),
		changes: vec![dep_change],
	};
	assert_eq!(file_result.path, "pubspec.yaml");
	assert_eq!(file_result.changes.len(), 1);
}

#[test]
fn version_map_construction() {
	let version_map: BTreeMap<String, String> = BTreeMap::from([
		("pkg_a".to_string(), "1.2.3".to_string()),
		("pkg_b".to_string(), "2.0.0".to_string()),
	]);
	assert_eq!(version_map.get("pkg_a"), Some(&"1.2.3".to_string()));
	assert_eq!(version_map.get("pkg_b"), Some(&"2.0.0".to_string()));
	assert_eq!(version_map.get("pkg_c"), None);
}
