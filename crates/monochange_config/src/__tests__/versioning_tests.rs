#![allow(clippy::disallowed_methods)]
use std::fs;

use rstest::rstest;
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
fn unknown_version_scheme_reference_lists_the_declared_schemes() {
	// When schemes exist, the diagnostic names them so a typo is obvious.
	let error = load_err(&format!(
		"{BASE}\ndisplay_version = \"missing\"\n\n[version_scheme.calver]\ntemplate = \"{{{{ year }}}}\"\n"
	));
	assert!(
		error.contains("unknown version scheme `missing`"),
		"{error}"
	);
	assert!(error.contains("declared schemes: calver"), "{error}");
}

#[test]
fn rejects_calendar_variables_in_the_package_manifest() {
	let error = load_err(&format!(
		"{BASE}\n\n[[package.app.versioned_files]]\npath = \"package.json\"\ntype = \"npm\"\nvalue_template = \"{{{{ year }}}}.{{{{ month }}}}\"\n"
	));
	assert!(error.contains("cannot render a valid `SemVer`"), "{error}");
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
fn rejects_malformed_value_ids() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.\"Bad-Id\"]\nenv = \"X\"\n"
	));
	assert!(error.contains("invalid value id"), "{error}");
}

#[test]
fn rejects_value_definitions_with_multiple_sources() {
	let error = load_err(&format!(
		"{BASE}\n\n[package.app.values.build]\nfile = \"b.json\"\nfield = \"build\"\nenv = \"BUILD\"\n"
	));
	assert!(error.contains("multiple sources"), "{error}");
}

fn title_configuration(scope: &str, key: &str, template: &str) -> String {
	let template = serde_json::to_string(template)
		.unwrap_or_else(|error| panic!("serialize title template: {error}"));
	let declaration = format!("{key} = {template}\n");

	let configuration = match scope {
		"defaults" => format!("[defaults]\n{declaration}\n{BASE}"),
		"group" => format!("{BASE}\n[group.sdk]\npackages = [\"app\"]\n{declaration}"),
		_ => format!("{BASE}\n{declaration}"),
	};

	format!("{configuration}\n[package.app.values.build]\nenv = \"BUILD_NUMBER\"\n")
}

#[rstest]
fn titles_accept_every_release_context_field(
	#[values("package", "group", "defaults")] scope: &str,
	#[values("release_title", "changelog_version_title")] key: &str,
) {
	let template = "{{ id }} {{ version }} {{ previous_version }} {{ date }} {{ time }} {{ datetime }} {{ changes_count }} {{ tag_url }} {{ compare_url }}";
	let configuration = load(&title_configuration(scope, key, template))
		.unwrap_or_else(|error| panic!("title context fields should load: {error}"));

	let titles = match scope {
		"defaults" => {
			(
				&configuration.defaults.release_title,
				&configuration.defaults.changelog_version_title,
			)
		}
		"group" => {
			let group = configuration
				.group_by_id("sdk")
				.unwrap_or_else(|| panic!("group sdk should exist"));
			(&group.release_title, &group.changelog_version_title)
		}
		_ => {
			let package = configuration
				.package_by_id("app")
				.unwrap_or_else(|| panic!("package app should exist"));
			(&package.release_title, &package.changelog_version_title)
		}
	};
	let title = if key == "release_title" {
		titles.0
	} else {
		titles.1
	};

	assert_eq!(title.as_deref(), Some(template));
}

#[rstest]
fn titles_reject_version_value_variables(
	#[values("package", "group", "defaults")] scope: &str,
	#[values("release_title", "changelog_version_title")] key: &str,
	#[values("name", "year", "build", "build.version")] variable: &str,
) {
	let error = load_err(&title_configuration(
		scope,
		key,
		&format!("{{{{ {variable} }}}}"),
	));
	assert!(error.contains("unknown variable"), "{error}");
	assert!(error.contains(&format!("{{{{ {variable} }}}}")), "{error}");
	assert!(error.contains("previous_version"), "{error}");
	assert!(error.contains(key), "{error}");
}

#[rstest]
fn titles_accept_jinja_expressions_and_static_text(
	#[values("package", "group", "defaults")] scope: &str,
	#[values("release_title", "changelog_version_title")] key: &str,
	#[values(
		"{% if previous_version %}{{ id | upper }} {{ version }}{% else %}{{ version }}{% endif %}",
		"Release notes"
	)]
	template: &str,
) {
	load(&title_configuration(scope, key, template))
		.unwrap_or_else(|error| panic!("valid Jinja title should load: {error}"));
}

#[rstest]
fn titles_reject_invalid_jinja_syntax(
	#[values("package", "group", "defaults")] scope: &str,
	#[values("release_title", "changelog_version_title")] key: &str,
	#[values("{{ version", "{% if previous_version %}{{ version }}")] template: &str,
) {
	let error = load_err(&title_configuration(scope, key, template));
	assert!(error.contains("invalid title template"), "{error}");
	assert!(error.contains(key), "{error}");
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
