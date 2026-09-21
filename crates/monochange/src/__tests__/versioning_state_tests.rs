#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::fs;

use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::versioning::LabelInputs;
use monochange_core::versioning::ReleaseTimestamp;
use monochange_core::versioning::ResetPolicy;
use monochange_core::versioning::StampBehaviour;
use monochange_core::versioning::ValueDefinition;
use tempfile::tempdir;

use crate::versioning_state::PackageValues;
use crate::versioning_state::ResolveContext;
use crate::versioning_state::ResolvedReleaseValues;
use crate::versioning_state::apply_counter_write_backs;
use crate::versioning_state::resolve_release_values;

/// Look up a resolved package entry, panicking with a clear message when the
/// package was not resolved.
fn package_entry<'a>(resolved: &'a ResolvedReleaseValues, id: &str) -> &'a PackageValues {
	resolved
		.packages
		.get(id)
		.unwrap_or_else(|| panic!("package `{id}` should be resolved"))
}

/// Look up a declared value string for a resolved package.
fn value_of(resolved: &ResolvedReleaseValues, package: &str, value: &str) -> String {
	package_entry(resolved, package)
		.values
		.get(value)
		.unwrap_or_else(|| panic!("value `{value}` should be resolved for `{package}`"))
		.clone()
}

fn value_definition(source: &str) -> ValueDefinition {
	toml::from_str(source).unwrap_or_else(|error| panic!("value definition should parse: {error}"))
}

fn package(id: &str, values: Vec<(&str, ValueDefinition)>) -> PackageDefinition {
	PackageDefinition {
		id: id.to_string(),
		path: std::path::PathBuf::from(id),
		package_type: PackageType::Npm,
		changelog: None,
		excluded_changelog_types: Vec::new(),
		bump_propagation: None,
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Namespaced,
		version_source: monochange_core::VersionSource::Manifest,
		initial_version: None,
		floating_tags: Vec::new(),
		bump_ceiling: None,
		classification_enforced: None,
		cli: None,
		values: values
			.into_iter()
			.map(|(id, definition)| (id.to_string(), definition))
			.collect(),
		display_version: None,
		publish: monochange_core::PublishSettings::default(),
	}
}

fn released(id: &str, version: &str) -> BTreeMap<String, String> {
	BTreeMap::from([(id.to_string(), version.to_string())])
}

struct Fixture {
	_dir: tempfile::TempDir,
	root: std::path::PathBuf,
}

fn fixture(counter_json: &str) -> Fixture {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path().to_path_buf();
	fs::write(root.join("build.json"), counter_json)
		.unwrap_or_else(|error| panic!("write counter: {error}"));
	Fixture { _dir: dir, root }
}

fn context<'a>(
	root: &'a std::path::Path,
	previous_version: Option<&'a str>,
	previous_inputs: Option<&'a LabelInputs>,
) -> ResolveContext<'a> {
	ResolveContext {
		root,
		timestamp: monochange_core::versioning::ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0)
			.unwrap_or_else(|error| panic!("timestamp: {error}")),
		previous_inputs,
		previous_version,
		commit: None,
		commit_timestamp: None,
	}
}

#[tokio::test]
async fn increments_a_counter_from_zero() {
	let f = fixture("{\"build\": 0}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"1"
	);
	assert!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.monotonic
	);
}

#[tokio::test]
async fn increments_an_existing_counter() {
	let f = fixture("{\"build\": 41}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"42"
	);
}

#[tokio::test]
async fn owner_scoped_counters_never_reset() {
	let f = fixture("{\"build\": 7}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\nreset = \"never\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, Some("1.0.0"), None),
		&packages,
		&BTreeMap::new(),
		&released("app", "2.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"8"
	);
}

#[tokio::test]
async fn train_scoped_counters_reset_when_the_version_changes() {
	let f = fixture("{\"build\": 3}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\nreset = \"version\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, Some("1.0.0"), None),
		&packages,
		&BTreeMap::new(),
		&released("app", "2.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"1"
	);
}

#[tokio::test]
async fn train_scoped_counters_continue_within_the_same_version() {
	let f = fixture("{\"build\": 3}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\nreset = \"version\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, Some("2.0.0"), None),
		&packages,
		&BTreeMap::new(),
		&released("app", "2.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"4"
	);
}

#[tokio::test]
async fn add_behaviour_steps_by_the_configured_amount() {
	let f = fixture("{\"build\": 100}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition(
				"file = \"build.json\"\nfield = \"build\"\non_release = { add = { amount = 10 } }\n",
			),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"110"
	);
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.write_backs[0]
			.value,
		110
	);
}

#[tokio::test]
async fn read_only_counters_do_not_write_back() {
	let f = fixture("{\"build\": 5}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\non_release = \"none\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"5"
	);
	assert!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.write_backs
			.is_empty()
	);
	assert!(
		!resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.monotonic
	);
}

#[tokio::test]
async fn reads_nested_counter_fields() {
	let f = fixture("{\"custom\": {\"data\": {\"build\": 9}}}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"custom.data.build\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.get("build")
			.unwrap_or_else(|| panic!("build value"))
			.clone(),
		"10"
	);
}

#[tokio::test]
async fn missing_counter_file_is_a_config_error() {
	let f = fixture("{}");
	fs::remove_file(f.root.join("build.json")).unwrap_or_else(|error| panic!("remove: {error}"));
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("missing file should fail");
	assert!(error.to_string().contains("does not exist"));
	assert!(
		error
			.to_string()
			.contains("create it with its starting value")
	);
}

#[tokio::test]
async fn missing_counter_field_is_a_config_error() {
	let f = fixture("{\"other\": 1}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("missing field should fail");
	assert!(error.to_string().contains("does not contain field"));
}

#[tokio::test]
async fn malformed_counter_json_is_a_config_error() {
	let f = fixture("not json");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("malformed json should fail");
	assert!(error.to_string().contains("could not parse counter file"));
}

#[tokio::test]
async fn non_integer_counter_is_a_config_error() {
	let f = fixture("{\"build\": \"four\"}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("non-integer should fail");
	assert!(error.to_string().contains("non-negative integers"));
}

#[tokio::test]
async fn hash_values_are_derived_and_not_monotonic() {
	let f = fixture("{}");
	fs::write(f.root.join("artifact.bin"), b"hello")
		.unwrap_or_else(|error| panic!("write artifact: {error}"));
	let packages = vec![package(
		"app",
		vec![(
			"artifact",
			value_definition("hash = \"artifact.bin\"\nlength = 8\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	let value = &value_of(&resolved, "app", "artifact");
	assert_eq!(value.len(), 8);
	assert!(value.chars().all(|c| c.is_ascii_hexdigit()));
	assert!(
		!resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.monotonic
	);
}

#[tokio::test]
async fn hash_digits_encoding_is_numeric_only() {
	let f = fixture("{}");
	fs::write(f.root.join("artifact.bin"), b"hello")
		.unwrap_or_else(|error| panic!("write artifact: {error}"));
	let packages = vec![package(
		"app",
		vec![(
			"artifact",
			value_definition("hash = \"artifact.bin\"\nencoding = \"digits\"\nlength = 6\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	let value = &value_of(&resolved, "app", "artifact");
	assert_eq!(value.len(), 6);
	assert!(value.chars().all(|c| c.is_ascii_digit()));
}

#[tokio::test]
async fn missing_hash_target_is_an_io_error() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("artifact", value_definition("hash = \"missing.bin\"\n"))],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("missing hash target should fail");
	assert!(error.to_string().contains("missing.bin"));
}

#[tokio::test]
async fn env_values_read_the_environment() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![(
			"run",
			value_definition("env = \"MONOCHANGE_TEST_RUN_NUMBER\"\n"),
		)],
	)];
	temp_env::async_with_vars([("MONOCHANGE_TEST_RUN_NUMBER", Some("77"))], async {
		let resolved = resolve_release_values(
			&context(&f.root, None, None),
			&packages,
			&BTreeMap::new(),
			&released("app", "1.0.0"),
		)
		.await
		.unwrap_or_else(|error| panic!("resolve: {error}"));
		assert_eq!(value_of(&resolved, "app", "run"), "77");
	})
	.await;
}

#[tokio::test]
async fn missing_env_variable_is_a_config_error() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("run", value_definition("env = \"MONOCHANGE_TEST_UNSET\"\n"))],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("missing env var should fail");
	assert!(error.to_string().contains("MONOCHANGE_TEST_UNSET"));
}

#[tokio::test]
async fn timestamp_values_render_the_release_moment() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("when", value_definition("timestamp = \"now\"\n"))],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(value_of(&resolved, "app", "when"), "20260919120000");
}

#[tokio::test]
async fn commit_timestamp_requires_a_commit() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("when", value_definition("timestamp = \"commit\"\n"))],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("commit timestamp without a commit should fail");
	assert!(error.to_string().contains("commit timestamp"));
}

#[tokio::test]
async fn git_values_require_a_commit() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("rev", value_definition("git = \"short_hash\"\n"))],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("git value without a commit should fail");
	assert!(error.to_string().contains("release commit"));
}

#[tokio::test]
async fn labels_render_the_configured_scheme() {
	let f = fixture("{\"build\": 4}");
	let mut packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	packages[0].display_version = Some("calver".to_string());
	let schemes = BTreeMap::from([(
		"calver".to_string(),
		monochange_core::versioning::VersionSchemeDefinition {
			template: "{{ year }}.{{ month }}.{{ release_of_month }}.{{ build }}".to_string(),
		},
	)]);
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&schemes,
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.label
			.as_deref(),
		Some("2026.9.1.5")
	);
}

#[tokio::test]
async fn ordinals_chain_from_the_previous_release() {
	let f = fixture("{}");
	let packages = vec![package("app", Vec::new())];
	let previous = LabelInputs {
		date: "2026-09-01".to_string(),
		time: "000000".to_string(),
		of_month: 2,
		of_quarter: 5,
		of_year: 9,
	};
	let resolved = resolve_release_values(
		&context(&f.root, None, Some(&previous)),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.label_inputs.of_month, 3);
	assert_eq!(resolved.label_inputs.of_quarter, 6);
	assert_eq!(resolved.label_inputs.of_year, 10);
}

#[tokio::test]
async fn packages_without_values_resolve_to_nothing() {
	let f = fixture("{}");
	let packages = vec![package("app", Vec::new())];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.values
			.is_empty()
	);
	assert!(
		resolved
			.packages
			.get("app")
			.unwrap_or_else(|| panic!("package entry"))
			.label
			.is_none()
	);
}

#[tokio::test]
async fn unreleased_packages_are_skipped() {
	let f = fixture("{\"build\": 1}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&BTreeMap::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert!(resolved.packages.is_empty());
}

#[tokio::test]
async fn unknown_display_scheme_is_a_config_error() {
	let f = fixture("{}");
	let mut packages = vec![package("app", Vec::new())];
	packages[0].display_version = Some("missing".to_string());
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.expect_err("unknown scheme should fail");
	assert!(error.to_string().contains("unknown version scheme"));
}

#[tokio::test]
async fn write_backs_rewrite_flat_counter_fields() {
	let f = fixture("{\n  \"build\": 4,\n  \"name\": \"app\"\n}\n");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 5,
	}];
	apply_counter_write_backs(&f.root, &write_backs)
		.await
		.unwrap_or_else(|error| panic!("write back: {error}"));
	let updated = fs::read_to_string(f.root.join("build.json"))
		.unwrap_or_else(|error| panic!("read: {error}"));
	assert!(updated.contains("\"build\": 5"));
	assert!(updated.contains("\"name\": \"app\""));
}

#[tokio::test]
async fn write_backs_rewrite_nested_counter_fields() {
	let f = fixture("{\"custom\": {\"build\": 4}}");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "custom.build".to_string(),
		value: 17,
	}];
	apply_counter_write_backs(&f.root, &write_backs)
		.await
		.unwrap_or_else(|error| panic!("write back: {error}"));
	let updated = fs::read_to_string(f.root.join("build.json"))
		.unwrap_or_else(|error| panic!("read: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&updated).unwrap_or_else(|error| panic!("parse: {error}"));
	assert_eq!(parsed["custom"]["build"], 17);
}

#[tokio::test]
async fn write_backs_report_missing_files() {
	let f = fixture("{}");
	fs::remove_file(f.root.join("build.json")).unwrap_or_else(|error| panic!("remove: {error}"));
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 1,
	}];
	let error = apply_counter_write_backs(&f.root, &write_backs)
		.await
		.expect_err("missing file should fail");
	assert!(error.to_string().contains("build.json"));
}

#[tokio::test]
async fn frozen_values_are_namespaced_by_package() {
	let f = fixture("{\"build\": 2}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.frozen_values()["app.build"], "3");
	assert!(!resolved.is_empty());
	assert_eq!(resolved.write_backs().len(), 1);
}

#[test]
fn stamp_behaviour_defaults_match_file_counters() {
	let increment = value_definition("file = \"b.json\"\nfield = \"build\"\n");
	assert_eq!(increment.stamp_behaviour(), StampBehaviour::Increment);
	assert_eq!(increment.reset, ResetPolicy::Never);
}

/// Build a context that has a commit available, for git-derived values.
fn context_with_commit<'a>(
	root: &'a std::path::Path,
	commit: &'a str,
	commit_timestamp: ReleaseTimestamp,
) -> ResolveContext<'a> {
	ResolveContext {
		root,
		timestamp: ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
		previous_inputs: None,
		previous_version: None,
		commit: Some(commit),
		commit_timestamp: Some(commit_timestamp),
	}
}

#[tokio::test]
async fn git_short_hash_reads_the_release_commit() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("rev", value_definition("git = \"short_hash\"\n"))],
	)];
	let context = context_with_commit(
		&f.root,
		"0123456789abcdef0123456789abcdef01234567",
		ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
	);
	let resolved = resolve_release_values(
		&context,
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.packages["app"].values["rev"], "0123456");
	assert!(!resolved.packages["app"].monotonic);
}

#[tokio::test]
async fn git_short_hash_tolerates_a_short_commit() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("rev", value_definition("git = \"short_hash\"\n"))],
	)];
	let context = context_with_commit(
		&f.root,
		"abc",
		ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
	);
	let resolved = resolve_release_values(
		&context,
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.packages["app"].values["rev"], "abc");
}

#[tokio::test]
async fn commit_timestamp_renders_when_available() {
	let f = fixture("{}");
	let packages = vec![package(
		"app",
		vec![("when", value_definition("timestamp = \"commit\"\n"))],
	)];
	let context = context_with_commit(
		&f.root,
		"abc1234",
		ReleaseTimestamp::new(2025, 1, 2, 3, 4, 5).unwrap_or_default(),
	);
	let resolved = resolve_release_values(
		&context,
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.packages["app"].values["when"], "20250102030405");
}

#[tokio::test]
async fn counter_files_that_are_not_objects_are_configuration_errors() {
	let f = fixture("[1, 2, 3]");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("an array counter file should fail"));
	assert!(error.to_string().contains("does not contain field"));
}

#[tokio::test]
async fn read_only_counters_report_a_missing_field() {
	let f = fixture("{\"other\": 1}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\non_release = \"none\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("a missing field should fail"));
	assert!(error.to_string().contains("does not contain field"));
}

#[tokio::test]
async fn value_definitions_without_a_source_are_configuration_errors() {
	let f = fixture("{}");
	// Reach the resolver directly with an empty definition, which the config
	// validator would normally reject first.
	let packages = vec![package("app", vec![("empty", ValueDefinition::default())])];
	let error = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("an empty definition should fail"));
	assert!(error.to_string().contains("declares no source"));
}

#[tokio::test]
async fn labels_are_frozen_alongside_values() {
	let f = fixture("{\"build\": 2}");
	let mut packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	packages[0].display_version = Some("calver".to_string());
	let schemes = BTreeMap::from([(
		"calver".to_string(),
		monochange_core::versioning::VersionSchemeDefinition {
			template: "{{ label }}".to_string(),
		},
	)]);
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&schemes,
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	// `label` resolves to the package's own rendering, so the frozen map is
	// keyed by package id.
	let labels = resolved.frozen_labels();
	assert_eq!(labels.len(), 1);
	assert!(labels.contains_key("app"));
}

#[tokio::test]
async fn write_backs_report_malformed_documents() {
	let f = fixture("not json");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 5,
	}];
	let error = apply_counter_write_backs(&f.root, &write_backs)
		.await
		.err()
		.unwrap_or_else(|| panic!("malformed json should fail"));
	assert!(error.to_string().contains("could not update counter field"));
}

#[tokio::test]
async fn write_backs_report_non_numeric_fields() {
	let f = fixture("{\"build\": \"four\"}");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 5,
	}];
	let error = apply_counter_write_backs(&f.root, &write_backs)
		.await
		.err()
		.unwrap_or_else(|| panic!("a non-numeric field should fail"));
	assert!(error.to_string().contains("does not hold a number"));
}

#[tokio::test]
async fn write_backs_report_nested_fields_that_are_missing() {
	let f = fixture("{\"custom\": {}}");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "custom.deep.build".to_string(),
		value: 5,
	}];
	let error = apply_counter_write_backs(&f.root, &write_backs)
		.await
		.err()
		.unwrap_or_else(|| panic!("a missing nested segment should fail"));
	assert!(error.to_string().contains("missing object segment"));
}

#[tokio::test]
async fn write_backs_report_paths_without_segments() {
	let f = fixture("{\"build\": 1}");
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: String::new(),
		value: 5,
	}];
	let error = apply_counter_write_backs(&f.root, &write_backs)
		.await
		.err()
		.unwrap_or_else(|| panic!("an empty field path should fail"));
	assert!(error.to_string().contains("no segments"));
}

#[tokio::test]
async fn values_for_returns_the_packages_values_and_empty_for_an_unknown_package() {
	let f = fixture("{\"build\": 3}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(
		resolved.values_for("app"),
		BTreeMap::from([("build".to_string(), "4".to_string())])
	);
	// The unknown-package branch returns an empty map rather than panicking.
	assert!(resolved.values_for("missing").is_empty());
}

#[tokio::test]
async fn write_backs_collects_counters_across_packages() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	fs::write(root.join("app.json"), "{\"build\": 1}")
		.unwrap_or_else(|error| panic!("write app counter: {error}"));
	fs::write(root.join("lib.json"), "{\"build\": 10}")
		.unwrap_or_else(|error| panic!("write lib counter: {error}"));
	let packages = vec![
		package(
			"app",
			vec![(
				"build",
				value_definition("file = \"app.json\"\nfield = \"build\"\n"),
			)],
		),
		package(
			"lib",
			vec![(
				"build",
				value_definition("file = \"lib.json\"\nfield = \"build\"\n"),
			)],
		),
	];
	let resolved = resolve_release_values(
		&context(root, None, None),
		&packages,
		&BTreeMap::new(),
		&BTreeMap::from([
			("app".to_string(), "1.0.0".to_string()),
			("lib".to_string(), "1.0.0".to_string()),
		]),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	let write_backs = resolved.write_backs();
	assert_eq!(write_backs.len(), 2);
	assert!(write_backs.iter().any(|write_back| write_back.value == 2));
	assert!(write_backs.iter().any(|write_back| write_back.value == 11));
}

#[tokio::test]
async fn git_commit_count_reads_the_repository() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	init_git_repo(root);
	fs::write(root.join("tracked.txt"), "one")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	commit_all(root, "first");
	fs::write(root.join("tracked.txt"), "two")
		.unwrap_or_else(|error| panic!("rewrite tracked: {error}"));
	commit_all(root, "second");
	let head = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);

	let packages = vec![package(
		"app",
		vec![("rev", value_definition("git = \"commit_count\"\n"))],
	)];
	let context = context_with_commit(
		root,
		&head,
		ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
	);
	let resolved = resolve_release_values(
		&context,
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.packages["app"].values["rev"], "2");
	assert!(!resolved.packages["app"].monotonic);
}

#[tokio::test]
async fn git_commit_count_reports_a_failure_for_an_unknown_revision() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	init_git_repo(root);
	fs::write(root.join("tracked.txt"), "one")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	commit_all(root, "first");

	let packages = vec![package(
		"app",
		vec![("rev", value_definition("git = \"commit_count\"\n"))],
	)];
	let context = context_with_commit(
		root,
		"0123456789abcdef0123456789abcdef01234567",
		ReleaseTimestamp::new(2026, 9, 19, 12, 0, 0).unwrap_or_default(),
	);
	let error = resolve_release_values(
		&context,
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("an unknown revision should fail"));
	assert!(
		error
			.to_string()
			.contains("count commits for the release value")
	);
}

fn init_git_repo(root: &std::path::Path) {
	git_in_temp_repo(root, &["init", "-b", "main"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
}

fn commit_all(root: &std::path::Path, message: &str) {
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", message]);
}

fn git_in_temp_repo(root: &std::path::Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_temp_repo(root: &std::path::Path, args: &[&str]) -> String {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}

#[tokio::test]
async fn train_reset_moves_the_counter_to_one_in_a_new_version() {
	let f = fixture("{\"build\": 7}");
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\nreset = \"version\"\n"),
		)],
	)];
	// A different previous identity resets a train-scoped counter.
	let resolved = resolve_release_values(
		&context(&f.root, Some("1.0.0"), None),
		&packages,
		&BTreeMap::new(),
		&released("app", "2.0.0"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	assert_eq!(resolved.packages["app"].values["build"], "1");
}

#[tokio::test]
async fn display_scheme_renders_prerelease_without_build_metadata() {
	let f = fixture("{}");
	let mut packages = vec![package("app", Vec::new())];
	packages[0].display_version = Some("identity".to_string());
	// `{{ prerelease }}` comes from the identity, so a prerelease version with
	// build metadata renders the prerelease tail without the metadata.
	let schemes = BTreeMap::from([(
		"identity".to_string(),
		monochange_core::versioning::VersionSchemeDefinition {
			template: "{{ prerelease }}".to_string(),
		},
	)]);
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&schemes,
		&released("app", "1.2.3-alpha.1+build.5"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	// `prerelease` carries the prerelease tail with build metadata removed.
	assert_eq!(resolved.packages["app"].label.as_deref(), Some("alpha.1"));
}

#[tokio::test]
async fn display_scheme_renders_an_empty_prerelease_for_a_stable_version() {
	let f = fixture("{}");
	let mut packages = vec![package("app", Vec::new())];
	packages[0].display_version = Some("identity".to_string());
	let schemes = BTreeMap::from([(
		"identity".to_string(),
		monochange_core::versioning::VersionSchemeDefinition {
			template: "{{ identity }}.{{ prerelease }}".to_string(),
		},
	)]);
	let resolved = resolve_release_values(
		&context(&f.root, None, None),
		&packages,
		&schemes,
		&released("app", "1.2.3"),
	)
	.await
	.unwrap_or_else(|error| panic!("resolve: {error}"));
	// A stable version has no prerelease, so the variable renders empty.
	assert_eq!(resolved.packages["app"].label.as_deref(), Some("1.2.3."));
}

#[tokio::test]
async fn counter_file_read_reports_a_non_missing_io_error() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	// A directory where the counter file is expected reads as an IO error that
	// is not `NotFound`, so it surfaces as an `IoSource` rather than the
	// missing-file guidance.
	fs::create_dir_all(root.join("build.json"))
		.unwrap_or_else(|error| panic!("create counter dir: {error}"));
	let packages = vec![package(
		"app",
		vec![(
			"build",
			value_definition("file = \"build.json\"\nfield = \"build\"\n"),
		)],
	)];
	let error = resolve_release_values(
		&context(root, None, None),
		&packages,
		&BTreeMap::new(),
		&released("app", "1.0.0"),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("a directory in place of the counter file should fail"));
	assert!(error.to_string().contains("io error"), "got: {error}");
}

#[tokio::test]
async fn counter_write_back_reports_a_non_missing_io_error() {
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	// The counter is read through a real file, then the directory is put in its
	// place so the write-back cannot replace it.
	fs::write(root.join("build.json"), "{\"build\": 1}")
		.unwrap_or_else(|error| panic!("write counter: {error}"));
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 2,
	}];
	fs::remove_file(root.join("build.json"))
		.unwrap_or_else(|error| panic!("remove counter: {error}"));
	fs::create_dir(root.join("build.json"))
		.unwrap_or_else(|error| panic!("create counter dir: {error}"));
	let error = apply_counter_write_backs(root, &write_backs)
		.await
		.err()
		.unwrap_or_else(|| panic!("writing into a directory should fail"));
	assert!(error.to_string().contains("io error"), "got: {error}");
}

#[tokio::test]
async fn write_backs_report_a_flat_field_without_a_value_separator() {
	let f = fixture("{\"build\" 3}");
	// The key exists but no colon follows it, so the flat rewrite cannot locate
	// the value and reports the field instead of writing garbage.
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 5,
	}];
	let error = apply_counter_write_backs(&f.root, &write_backs)
		.await
		.err()
		.unwrap_or_else(|| panic!("a field without a separator should fail"));
	assert!(error.to_string().contains("has no value"), "got: {error}");
}

#[cfg(unix)]
#[tokio::test]
async fn counter_write_back_reports_a_write_failure() {
	use std::os::unix::fs::PermissionsExt;
	let dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = dir.path();
	let counter = root.join("build.json");
	fs::write(&counter, "{\"build\": 1}").unwrap_or_else(|error| panic!("write counter: {error}"));
	// The read succeeds, then the file is made read-only so the write fails.
	fs::set_permissions(&counter, fs::Permissions::from_mode(0o444))
		.unwrap_or_else(|error| panic!("chmod counter: {error}"));
	let write_backs = vec![monochange_core::versioning::CounterWriteBack {
		file: std::path::PathBuf::from("build.json"),
		field: "build".to_string(),
		value: 2,
	}];
	let result = apply_counter_write_backs(root, &write_backs).await;
	// Restore permissions so the temp directory can be cleaned up.
	let _ = fs::set_permissions(&counter, fs::Permissions::from_mode(0o644));
	let error = result
		.err()
		.unwrap_or_else(|| panic!("writing a read-only counter should fail"));
	assert!(error.to_string().contains("io error"), "got: {error}");
}
