use serde_json::Value;
use serde_json::json;

use crate::CURRENT_SCHEMA_VERSION_TEXT;
use crate::SchemaError;
use crate::SchemaVersion;
use crate::SchemaVersionParseError;
use crate::config;
use crate::current_schema_version;
use crate::migrations;
use crate::release_record;

#[test]
fn schema_version_parses_major_minor_only() {
	let version: SchemaVersion = "8.2"
		.parse()
		.unwrap_or_else(|error| panic!("parse schema version: {error}"));
	assert_eq!(version.major(), 8);
	assert_eq!(version.minor(), 2);
	assert_eq!(version.to_string(), "8.2");
	assert!("8.2.1".parse::<SchemaVersion>().is_err());
	assert!("8".parse::<SchemaVersion>().is_err());
	assert!("8.x".parse::<SchemaVersion>().is_err());
}

#[test]
fn package_version_parser_reports_component_errors() {
	assert!(matches!(
		SchemaVersion::from_package_version(""),
		Err(SchemaVersionParseError::MissingMinor)
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1"),
		Err(SchemaVersionParseError::MissingMinor)
	));
	assert!(matches!(
		SchemaVersion::from_package_version("x.2.3"),
		Err(SchemaVersionParseError::InvalidMajor(major)) if major == "x"
	));
	assert!(matches!(
		SchemaVersion::from_package_version(".2.3"),
		Err(SchemaVersionParseError::InvalidMajor(major)) if major.is_empty()
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1.x.3"),
		Err(SchemaVersionParseError::InvalidMinor(minor)) if minor == "x"
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1.2.x"),
		Err(SchemaVersionParseError::InvalidPatch(patch)) if patch == "x"
	));
	assert!(matches!(
		SchemaVersion::from_package_version("1."),
		Err(SchemaVersionParseError::MissingPatch)
	));
}

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
fn release_record_accepts_current_schema_version() {
	let migrated = release_record::migrate_value(json!({
		"schema_version": CURRENT_SCHEMA_VERSION_TEXT,
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.unwrap_or_else(|error| panic!("validate release record: {error}"));

	assert_eq!(
		migrated.get("schema_version"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
}

#[test]
fn release_record_migrates_older_schema_versions() {
	let migrated = release_record::migrate_value(json!({
		"schema_version": "0.1",
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.unwrap_or_else(|error| panic!("migrate old release record: {error}"));

	assert_eq!(
		migrated.get("schema_version"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
}

#[test]
fn release_record_migrates_legacy_camel_schema_version() {
	let migrated = release_record::migrate_value(json!({
		"schemaVersion": "0.3",
		"kind": release_record::KIND,
		"createdAt": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"releaseTargets": [{
			"id": "core",
			"kind": "package",
			"tagName": "core/v1.0.0",
			"versionFormat": "namespaced"
		}],
		"releasedPackages": [],
		"changedFiles": []
	}))
	.unwrap_or_else(|error| panic!("migrate legacy camel release record: {error}"));

	assert_eq!(
		migrated.get("schema_version"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
	assert!(migrated.get("schemaVersion").is_none());
	assert!(migrated.get("created_at").is_some());
	assert!(migrated.get("createdAt").is_none());
	assert!(migrated["release_targets"][0].get("tag_name").is_some());
	assert!(migrated["release_targets"][0].get("tagName").is_none());
}

#[test]
fn release_record_migrates_legacy_v_only_schema_version() {
	let migrated = release_record::migrate_value(json!({
		"v": "0.0",
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.unwrap_or_else(|error| panic!("migrate legacy release record: {error}"));

	assert_eq!(
		migrated.get("schema_version"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
	assert!(migrated.get("v").is_none());
}

#[test]
fn release_record_rust_migration_helpers_apply_supported_changes() {
	let mut value = json!({
		"oldName": "kept",
		"removed": true,
		"other": "stable"
	});

	migrations::rename_top_level_field(&mut value, "oldName", "newName")
		.unwrap_or_else(|error| panic!("rename field: {error}"));
	migrations::remove_top_level_field(&mut value, "removed")
		.unwrap_or_else(|error| panic!("remove field: {error}"));

	assert_eq!(value.get("newName"), Some(&json!("kept")));
	assert!(value.get("oldName").is_none());
	assert!(value.get("removed").is_none());
	assert_eq!(value.get("other"), Some(&json!("stable")));
}

#[test]
fn release_record_rust_migration_edges_are_explicit_and_ordered() {
	assert_eq!(
		migrations::release_record_edge_versions(),
		&[
			(SchemaVersion::new(0, 0), SchemaVersion::new(0, 1)),
			(SchemaVersion::new(0, 1), SchemaVersion::new(0, 2)),
			(SchemaVersion::new(0, 2), SchemaVersion::new(0, 3)),
			(SchemaVersion::new(0, 3), SchemaVersion::new(0, 4)),
			(SchemaVersion::new(0, 4), SchemaVersion::new(0, 5)),
		]
	);
}

#[test]
fn release_record_v0_4_migration_adds_default_output_identity() {
	let migrated = release_record::migrate_value(json!({
		"schema_version": "0.4",
		"kind": release_record::KIND,
		"changelogs": [
			{
				"owner_id": "app",
				"owner_kind": "package",
				"path": "CHANGELOG.md",
				"format": "monochange",
				"notes": { "title": "1.2.0", "sections": [] },
				"rendered": "## 1.2.0"
			},
			{
				"owner_id": "app",
				"owner_kind": "package",
				"output": "user",
				"stream": "user",
				"path": "release-notes.json",
				"format": "json",
				"notes": { "title": "1.2.0", "sections": [] },
				"rendered": "{}"
			},
			"ignored malformed entry"
		],
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.unwrap_or_else(|error| panic!("migrate v0.4 release record: {error}"));

	assert_eq!(migrated["schema_version"], json!("0.5"));
	assert_eq!(migrated["changelogs"][0]["output"], json!("default"));
	assert_eq!(migrated["changelogs"][0]["stream"], json!("default"));
	assert_eq!(migrated["changelogs"][1]["output"], json!("user"));
	assert_eq!(migrated["changelogs"][1]["stream"], json!("user"));
	assert_eq!(migrated["changelogs"][2], json!("ignored malformed entry"));
}

#[test]
fn release_record_rust_migration_edges_reject_missing_paths() {
	let mut value = json!({
		"kind": release_record::KIND,
		"schema_version": "0.5"
	});
	let error = migrations::apply_release_record_edges(
		&mut value,
		SchemaVersion::new(0, 5),
		SchemaVersion::new(0, 6),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing migration path error"));

	assert!(matches!(
		error,
		SchemaError::MissingMigrationPath {
			artifact: release_record::KIND,
			from: SchemaVersion { major: 0, minor: 5 },
			to: SchemaVersion { major: 0, minor: 6 },
		}
	));
}

#[test]
fn release_record_rust_migration_edges_reject_overshooting_paths() {
	let mut value = json!({
		"kind": release_record::KIND,
		"schema_version": "0.1"
	});
	let error = migrations::apply_release_record_edges(
		&mut value,
		SchemaVersion::new(0, 1),
		SchemaVersion::new(0, 0),
	)
	.err()
	.unwrap_or_else(|| panic!("expected overshooting migration path error"));

	assert!(matches!(
		error,
		SchemaError::MissingMigrationPath {
			artifact: release_record::KIND,
			from: SchemaVersion { major: 0, minor: 1 },
			to: SchemaVersion { major: 0, minor: 0 },
		}
	));
}

#[test]
fn release_record_rust_migration_helpers_reject_non_object_values() {
	let mut value = json!(null);
	let error = migrations::rename_top_level_field(&mut value, "oldName", "newName")
		.err()
		.unwrap_or_else(|| panic!("expected non-object rename error"));
	assert!(matches!(error, SchemaError::NotObject));

	let error = migrations::remove_top_level_field(&mut value, "removed")
		.err()
		.unwrap_or_else(|| panic!("expected non-object remove error"));
	assert!(matches!(error, SchemaError::NotObject));
}

#[test]
fn release_record_render_current_value_writes_public_version_only() {
	let rendered = release_record::render_current_value(json!({
		"schema_version": 1,
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.unwrap_or_else(|error| panic!("render current release record: {error}"));

	assert_eq!(
		rendered.get("schema_version"),
		Some(&json!(CURRENT_SCHEMA_VERSION_TEXT))
	);
	assert!(
		rendered.get("v").is_none(),
		"legacy `v` must not leak into durable records"
	);
}

#[test]
fn release_record_render_current_value_rejects_non_object_or_missing_kind() {
	let not_object = release_record::render_current_value(json!([]))
		.err()
		.unwrap_or_else(|| panic!("expected non-object error"));
	assert!(matches!(not_object, SchemaError::NotObject));

	let missing_kind = release_record::render_current_value(json!({
		"schema_version": 1,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected missing-kind error"));
	assert!(matches!(missing_kind, SchemaError::MissingKind));
}

#[test]
fn release_record_rejects_missing_version() {
	let error = release_record::migrate_value(json!({
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected missing version error"));
	assert!(matches!(error, SchemaError::MissingVersion));
}

#[test]
fn release_record_rejects_non_string_version() {
	let error = release_record::migrate_value(json!({
		"schema_version": 1,
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected non-string version error"));
	assert!(matches!(error, SchemaError::NonStringVersion));
}

#[test]
fn release_record_rejects_invalid_version_text() {
	let error = release_record::migrate_value(json!({
		"schema_version": "0.1.0",
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(matches!(
		error,
		SchemaError::InvalidVersion { version, .. } if version == "0.1.0"
	));
}

#[test]
fn release_record_rejects_unsupported_kind() {
	let error = release_record::migrate_value(json!({
		"schema_version": "0.1",
		"kind": "monochange.otherRecord",
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected unsupported kind error"));
	assert!(matches!(
		error,
		SchemaError::UnsupportedKind { actual, expected }
			if actual == "monochange.otherRecord" && expected == release_record::KIND
	));
}

#[test]
fn release_record_rejects_future_version() {
	let error = release_record::migrate_value(json!({
		"schema_version": "9.0",
		"kind": release_record::KIND,
		"created_at": "2026-04-06T12:00:00Z",
		"command": "release-pr",
		"release_targets": [],
		"released_packages": [],
		"changed_files": []
	}))
	.err()
	.unwrap_or_else(|| panic!("expected unsupported version error"));
	assert!(matches!(
		error,
		SchemaError::UnsupportedVersion { actual, .. } if actual == "9.0"
	));
}

#[test]
fn committed_release_record_schema_tracks_current_wire_constants() {
	let release_record_schema = include_str!("../../schemas/release-record.schema.json");
	let schema = serde_json::from_str::<Value>(release_record_schema)
		.unwrap_or_else(|error| panic!("release record schema json: {error}"));

	assert_eq!(
		schema
			.pointer("/properties/schema_version/default")
			.and_then(Value::as_str),
		Some(CURRENT_SCHEMA_VERSION_TEXT)
	);
	assert_eq!(
		schema
			.pointer("/properties/kind/const")
			.and_then(Value::as_str),
		Some(release_record::KIND)
	);
	assert_eq!(
		schema
			.pointer("/additionalProperties")
			.and_then(Value::as_bool),
		Some(false)
	);
}

#[test]
fn committed_json_schema_files_parse() {
	let release_record_schema = include_str!("../../schemas/release-record.schema.json");
	serde_json::from_str::<Value>(release_record_schema)
		.unwrap_or_else(|error| panic!("release record schema json: {error}"));
}
