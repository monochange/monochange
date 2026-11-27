use serde_json::Value;
use serde_json::json;

use crate::CURRENT_SCHEMA_VERSION_TEXT;
use crate::SchemaError;
use crate::SchemaVersion;
use crate::config;
use crate::current_schema_version;
use crate::migrations;
use crate::release_record;

#[test]
fn current_schema_version_is_not_behind_package_version() {
	let package_version = env!("CARGO_PKG_VERSION");
	let current = current_schema_version()
		.unwrap_or_else(|error| panic!("parse current schema version: {error}"));
	let package = SchemaVersion::from_package_version(package_version)
		.unwrap_or_else(|error| panic!("parse package version: {error}"));
	assert!(
		current >= package,
		"current durable schema {current} must not lag package-derived schema {package}"
	);
	let serialized = serde_json::to_value(current)
		.unwrap_or_else(|error| panic!("serialize schema version: {error}"));
	assert_eq!(serialized, json!(CURRENT_SCHEMA_VERSION_TEXT));
	assert_eq!(
		serde_json::to_value(package).unwrap(),
		json!(package.to_string())
	);
}

#[test]
fn populated_release_record_artifact_uses_current_schema_version() {
	let version = CURRENT_SCHEMA_VERSION_TEXT;
	let json = release_record::current_populated_artifact_json();
	let value: Value = serde_json::from_str(&json)
		.unwrap_or_else(|error| panic!("parse populated release record artifact: {error}"));

	assert_eq!(value["schema_version"], version);
	assert_eq!(value["kind"], release_record::KIND);
	assert_eq!(value["release_targets"].as_array().unwrap().len(), 2);
	assert_eq!(value["changesets"].as_array().unwrap().len(), 1);
	assert!(
		value["changed_files"]
			.as_array()
			.unwrap()
			.iter()
			.any(|entry| {
				entry
					== &json!(
						"crates/monochange_schema/schemas/artifacts/current/release-record/01.json"
					)
			})
	);
	assert!(
		!value["changed_files"]
			.as_array()
			.unwrap()
			.iter()
			.any(|entry| {
				entry
					.as_str()
					.is_some_and(|entry| entry.contains("release-record.v"))
			})
	);
}

#[test]
fn populated_config_artifact_is_deterministic() {
	let first = config::populated_artifact_json();
	let second = config::populated_artifact_json();
	assert_eq!(first, second);
	let value: Value = serde_json::from_str(&first)
		.unwrap_or_else(|error| panic!("parse populated config artifact: {error}"));
	assert_eq!(value["source"]["owner"], "monochange");
	assert_eq!(value["source"]["repo"], "monochange");
}

#[test]
fn release_record_rust_migration_edges_reject_missing_paths() {
	let current =
		current_schema_version().unwrap_or_else(|error| panic!("current schema version: {error}"));
	let (major, minor) = (current.major(), current.minor());
	let mut value = json!({
		"kind": release_record::KIND,
		"schema_version": CURRENT_SCHEMA_VERSION_TEXT
	});
	let error = migrations::apply_release_record_edges(
		&mut value,
		SchemaVersion::new(major, minor),
		SchemaVersion::new(major, minor + 1),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing migration path error"));

	assert!(matches!(
		error,
		SchemaError::MissingMigrationPath {
			artifact: release_record::KIND,
			from: SchemaVersion { major: found_major, minor: found_minor },
			to: SchemaVersion { major: target_major, minor: target_minor },
		} if found_major == major
			&& found_minor == minor
			&& target_major == major
			&& target_minor == minor + 1
	));
}
