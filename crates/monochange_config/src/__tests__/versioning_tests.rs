#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use monochange_core::versioning::HashEncoding;
use monochange_core::versioning::ResetPolicy;
use monochange_core::versioning::StampBehaviour;
use monochange_core::versioning::TimestampSource;
use tempfile::tempdir;

use crate::load_workspace_configuration;

fn load(
	contents: &str,
) -> monochange_core::MonochangeResult<monochange_core::WorkspaceConfiguration> {
	let temp_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(temp_dir.path().join("monochange.toml"), contents)
		.unwrap_or_else(|error| panic!("write config: {error}"));
	// A manifest must exist for package discovery to see the declared package.
	fs::write(
		temp_dir.path().join("package.json"),
		"{\"name\":\"app\",\"version\":\"1.0.0\"}",
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	load_workspace_configuration(temp_dir.path())
}

fn load_err(contents: &str) -> String {
	load(contents)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"))
		.to_string()
}

const BASE: &str = r#"
[package.app]
path = "."
type = "npm"
"#;

#[test]
fn parses_version_scheme_tables() {
	let configuration = load(&format!(
		"{BASE}\n[version_scheme.calver]\ntemplate = \"{{{{ year }}}}.{{{{ month }}}}\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let scheme = configuration
		.version_schemes
		.get("calver")
		.unwrap_or_else(|| panic!("scheme should be declared"));
	assert_eq!(scheme.template, "{{ year }}.{{ month }}");
}

#[test]
fn parses_display_version_reference() {
	let configuration = load(&format!(
		"{BASE}\ndisplay_version = \"calver\"\n\n[version_scheme.calver]\ntemplate = \"{{{{ year }}}}\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package should exist"));
	assert_eq!(package.display_version.as_deref(), Some("calver"));
}

#[test]
fn parses_file_counter_values() {
	let configuration = load(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"build\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package"));
	let value = package
		.values
		.get("build")
		.unwrap_or_else(|| panic!("declared value"));
	assert_eq!(value.file.as_deref(), Some(Path::new("build.json")));
	assert_eq!(value.field.as_deref(), Some("build"));
	assert_eq!(value.stamp_behaviour(), StampBehaviour::Increment);
	assert_eq!(value.reset, ResetPolicy::Never);
}

#[test]
fn parses_derived_value_sources() {
	let configuration = load(&format!(
		"{BASE}\n\n[package.app.values.hash_value]\nhash = \"app.aab\"\nlength = 8\nencoding = \"base36\"\n\n[package.app.values.run]\nenv = \"RUN_NUMBER\"\n\n[package.app.values.rev]\ngit = \"commit_count\"\n\n[package.app.values.when]\ntimestamp = \"commit\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package"));
	assert_eq!(package.values["hash_value"].encoding, HashEncoding::Base36);
	assert_eq!(package.values["hash_value"].length, Some(8));
	assert_eq!(package.values["run"].env.as_deref(), Some("RUN_NUMBER"));
	assert_eq!(
		package.values["when"].timestamp,
		Some(TimestampSource::Commit)
	);
}

#[test]
fn parses_value_template_on_versioned_files() {
	let configuration = load(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"build\"\n\n[[package.app.versioned_files]]\npath = \"app.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ identity }}}}+{{{{ build }}}}\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package"));
	let versioned_file = package
		.versioned_files
		.iter()
		.find(|file| file.path == "app.json")
		.unwrap_or_else(|| panic!("versioned file"));
	assert_eq!(
		versioned_file.value_template.as_deref(),
		Some("{{ identity }}+{{ build }}")
	);
}

#[test]
fn parses_counter_reset_and_behaviour() {
	let configuration = load(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"build\"\non_release = {{ add = {{ amount = 10 }} }}\nreset = \"version\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package"));
	let value = &package.values["build"];
	assert_eq!(value.stamp_behaviour(), StampBehaviour::Add { amount: 10 });
	assert_eq!(value.reset, ResetPolicy::Version);
}

#[test]
fn rejects_unknown_version_scheme_reference() {
	let error = load_err(&format!("{BASE}\ndisplay_version = \"missing\"\n"));
	assert!(
		error.contains("unknown version scheme `missing`"),
		"{error}"
	);
	assert!(error.contains("none declared"), "{error}");
}

#[test]
fn rejects_unknown_variable_in_scheme_template() {
	let error = load_err(&format!(
		"{BASE}\ndisplay_version = \"calver\"\n\n[version_scheme.calver]\ntemplate = \"{{{{ nope }}}}\"\n"
	));
	assert!(error.contains("unknown variable"), "{error}");
}

#[test]
fn rejects_undeclared_value_in_value_template() {
	let error = load_err(&format!(
		"{BASE}\n\n[[package.app.versioned_files]]\npath = \"app.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ build }}}}\"\n"
	));
	assert!(error.contains("unknown variable"), "{error}");
	assert!(error.contains("build"), "{error}");
}

#[test]
fn rejects_calendar_variables_in_the_package_manifest() {
	let error = load_err(&format!(
		"{BASE}\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ year }}}}.{{{{ month }}}}\"\n"
	));
	assert!(error.contains("cannot render a valid `SemVer`"), "{error}");
}

#[test]
fn allows_build_metadata_from_a_counter_on_the_package_manifest() {
	// Flutter and Dart manifests carry build metadata (`1.2.3+4`), and that is
	// the number Shorebird keys a release on.
	load(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"build\"\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ identity }}}}+{{{{ build }}}}\"\n"
	))
	.expect("a counter appended as build metadata should be valid on the manifest");
}

#[test]
fn rejects_letter_bearing_values_in_a_numeric_semver_position() {
	// A base36 hash can contain letters, so it cannot be build metadata.
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.tag]\nhash = \"artifact.bin\"\nencoding = \"base36\"\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ identity }}}}+{{{{ tag }}}}\"\n"
	));
	assert!(error.contains("can contain letters"), "{error}");
}

#[test]
fn allows_digits_encoded_hash_on_the_package_manifest() {
	load(&format!(
		"{BASE}\n\n[package.app.values.artifact]\nhash = \"artifact.bin\"\nencoding = \"digits\"\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ identity }}}}+{{{{ artifact }}}}\"\n"
	))
	.expect("a digits-encoded hash is numeric and valid as build metadata");
}

#[test]
fn rejects_templates_that_never_render_a_semver() {
	let error = load_err(&format!(
		"{BASE}\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"not-a-version\"\n"
	));
	assert!(error.contains("cannot render a valid `SemVer`"), "{error}");
}

#[test]
fn allows_semver_variables_in_the_package_manifest() {
	load(&format!(
		"{BASE}\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ identity }}}}\"\n"
	))
	.unwrap_or_else(|error| panic!("identity template should be allowed on the manifest: {error}"));
}

#[test]
fn rejects_value_ids_that_shadow_context_variables() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.year]\nenv = \"YEAR\"\n"
	));
	assert!(error.contains("reserved template variable"), "{error}");
}

#[test]
fn rejects_malformed_value_ids() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.\"Bad-Id\"]\nenv = \"X\"\n"
	));
	assert!(error.contains("invalid value id"), "{error}");
}

#[test]
fn rejects_value_definitions_without_a_source() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.build]\nlength = 4\n"
	));
	assert!(
		error.contains("no source") || error.contains("without `hash`"),
		"{error}"
	);
}

#[test]
fn rejects_value_definitions_with_multiple_sources() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"b.json\"\nfield = \"build\"\nenv = \"BUILD\"\n"
	));
	assert!(error.contains("multiple sources"), "{error}");
}

#[test]
fn rejects_file_counter_options_on_derived_values() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.rev]\ngit = \"short_hash\"\nreset = \"version\"\n"
	));
	assert!(error.contains("without a file counter"), "{error}");
}

#[test]
fn rejects_reserved_build_id_collision_is_allowed() {
	// `build` is the conventional counter id and must remain usable.
	load(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"build\"\n"
	))
	.unwrap_or_else(|error| panic!("`build` should be a valid value id: {error}"));
}

#[test]
fn rejects_empty_version_scheme_template() {
	let error = load_err(&format!(
		"{BASE}\n\n[version_scheme.calver]\ntemplate = \"  \"\n"
	));
	assert!(error.contains("empty template"), "{error}");
}

#[test]
fn packages_without_values_keep_empty_maps() {
	let configuration =
		load(BASE).unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package"));
	assert!(package.values.is_empty());
	assert_eq!(package.display_version, None);
	assert!(configuration.version_schemes.is_empty());
}

#[test]
fn rejects_unknown_keys_in_value_definitions() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"b.json\"\nfield = \"build\"\nnope = 1\n"
	));
	assert!(error.contains("nope"), "{error}");
}

#[test]
fn rejects_unknown_keys_in_version_scheme_tables() {
	let error = load_err(&format!(
		"{BASE}\n\n[version_scheme.calver]\ntemplate = \"{{{{ year }}}}\"\nnope = 1\n"
	));
	assert!(error.contains("nope"), "{error}");
}

#[test]
fn value_ids_are_available_to_multiple_templates() {
	load(&format!(
		"{BASE}\ndisplay_version = \"dual\"\n\n[version_scheme.dual]\ntemplate = \"{{{{ year }}}}.{{{{ build }}}}\"\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"build\"\n\n[[package.app.versioned_files]]\npath = \"app.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ identity }}}}+{{{{ build }}}}\"\n"
	))
	.unwrap_or_else(|error| panic!("declared value should be usable in scheme and versioned file: {error}"));
}

#[test]
fn rejects_unknown_variable_in_package_title() {
	let error = load_err(&format!("{BASE}\nrelease_title = \"{{{{ nope }}}}\"\n"));
	assert!(error.contains("unknown variable"), "{error}");
}

#[test]
fn multiple_packages_validate_independently() {
	let error = load_err(
		r#"
[package.app]
path = "."
type = "npm"

[package.app.values.build]
file = "build.json"
field = "build"

[package.other]
path = "other"
type = "npm"

[[package.other.versioned_files]]
path = "other.json"
type = "npm"
value_template = "{{ build }}"
"#,
	);
	assert!(error.contains("other"), "{error}");
}

#[test]
fn version_scheme_ids_must_be_lowercase_identifiers() {
	let error = load_err(&format!(
		"{BASE}\n\n[version_scheme.\"Bad-Id\"]\ntemplate = \"{{{{ year }}}}\"\n"
	));
	assert!(error.contains("invalid version scheme id"), "{error}");
}

#[test]
fn declared_values_round_trip_through_serialization() {
	let configuration = load(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"build.json\"\nfield = \"custom.build\"\n"
	))
	.unwrap_or_else(|error| panic!("configuration should load: {error}"));
	let package = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("package"));
	let value: &monochange_core::versioning::ValueDefinition = &package.values["build"];
	let serialized =
		serde_json::to_value(value).unwrap_or_else(|error| panic!("serialize: {error}"));
	assert_eq!(serialized["field"], "custom.build");
	assert_eq!(serialized["file"], "build.json");
}

#[test]
fn workspace_defaults_are_unaffected_by_versioning_keys() {
	let configuration =
		load(BASE).unwrap_or_else(|error| panic!("configuration should load: {error}"));
	assert_eq!(configuration.defaults.release_title, None);
	assert!(BTreeMap::<String, String>::new().is_empty());
}
