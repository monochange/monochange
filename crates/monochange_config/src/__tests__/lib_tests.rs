mod versioning_tests;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use miette::LabeledSpan;
use monochange_core::AutoDiscoverPackageDefaults;
use monochange_core::AutoDiscoverSettings;
use monochange_core::BumpSeverity;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogSettings;
use monochange_core::ChangelogTarget;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::EcosystemRegistry;
use monochange_core::EcosystemType;
use monochange_core::FloatingTagFormat;
use monochange_core::GroupChangelogInclude;
use monochange_core::GroupDefinition;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::RegistryKind;
use monochange_core::ShellConfig;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use monochange_core::VersionSource;
use monochange_core::lint::ChangesetLintSettings;
use monochange_core::lint::ChangesetScopedLintSettings;
use monochange_core::lint::ChangesetSummaryLintSettings;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintSeverity;
use monochange_test_helpers::snapshot_settings;
use semver::Version;
use tempfile::tempdir;

use crate::apply_version_groups;
use crate::cli_input_kind_matches_step_input;
use crate::frontmatter_span_for_line_column;
use crate::line_and_column_for_offset;
use crate::line_index_for_offset;
use crate::load_change_signals;
use crate::load_changeset_file;
use crate::load_workspace_configuration;
use crate::range_to_span;
use crate::render_diagnostic_notes;
use crate::render_source_diagnostic;
use crate::render_source_snippet;
use crate::render_source_snippets;
use crate::resolve_package_reference;
use crate::sort_labels_by_location;
use crate::validate_step_input_overrides;
use crate::validate_step_override_kind;

fn validate_package_and_group_definitions_for_test(
	root: &Path,
	config_contents: &str,
	packages: &[monochange_core::PackageDefinition],
	groups: &[GroupDefinition],
) -> MonochangeResult<()> {
	let declared_packages = packages
		.iter()
		.map(|package| package.id.as_str())
		.collect::<std::collections::BTreeSet<_>>();
	let mut cache = crate::VersionedFileValidationCache::default();
	crate::validate_package_and_group_definitions_with_cache(
		root,
		config_contents,
		packages,
		groups,
		&declared_packages,
		&mut cache,
	)
}

fn validate_versioned_files_for_test(
	root: &Path,
	config_contents: &str,
	versioned_files: &[monochange_core::VersionedFileDefinition],
	declared_packages: &std::collections::BTreeSet<&str>,
	owner_kind: &str,
	owner_id: &str,
) -> MonochangeResult<()> {
	let mut cache = crate::VersionedFileValidationCache::default();
	crate::validate_versioned_files_with_cache(
		root,
		config_contents,
		versioned_files,
		declared_packages,
		owner_kind,
		owner_id,
		&mut cache,
	)
}

fn validate_step() -> CliStepDefinition {
	CliStepDefinition::Validate {
		name: None,
		when: None,
		always_run: false,
		inputs: BTreeMap::new(),
	}
}

#[test]
fn cli_step_override_validation_covers_type_helpers_and_missing_inherited_inputs() {
	let command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![
			cli_input("fix", CliInputKind::Boolean),
			cli_input("format", CliInputKind::Choice),
		],
		steps: Vec::new(),
		dry_run: false,
	};
	let validate = validate_step();
	validate_step_override_kind(
		&command,
		&validate,
		"fix",
		Some(&CliStepInputValue::Boolean(true)),
		true,
	)
	.unwrap_or_else(|error| panic!("boolean override should validate: {error}"));
	validate_step_override_kind(
		&command,
		&validate,
		"fix",
		Some(&CliStepInputValue::String("{{ inputs.fix }}".to_string())),
		true,
	)
	.unwrap_or_else(|error| panic!("boolean template override should validate: {error}"));
	validate_step_override_kind(
		&command,
		&validate,
		"format",
		Some(&CliStepInputValue::String("json".to_string())),
		false,
	)
	.unwrap_or_else(|error| panic!("string override should validate: {error}"));
	validate_step_override_kind(
		&command,
		&validate,
		"format",
		Some(&CliStepInputValue::List(vec!["json".to_string()])),
		false,
	)
	.unwrap_or_else(|error| panic!("list override should validate: {error}"));
	assert!(
		validate_step_override_kind(
			&command,
			&validate,
			"fix",
			Some(&CliStepInputValue::List(vec!["true".to_string()])),
			true,
		)
		.is_err()
	);
	assert!(
		validate_step_override_kind(
			&command,
			&validate,
			"format",
			Some(&CliStepInputValue::Boolean(true)),
			false,
		)
		.is_err()
	);

	assert!(cli_input_kind_matches_step_input(
		CliInputKind::String,
		CliInputKind::Choice,
	));
	assert!(cli_input_kind_matches_step_input(
		CliInputKind::Path,
		CliInputKind::String,
	));
	assert!(cli_input_kind_matches_step_input(
		CliInputKind::Choice,
		CliInputKind::Path,
	));

	let mut inherited_inputs = BTreeMap::new();
	inherited_inputs.insert("format".to_string(), CliStepInputValue::Inherited);
	let step = CliStepDefinition::PrepareRelease {
		name: None,
		when: None,
		always_run: false,
		allow_empty_changesets: false,
		inputs: inherited_inputs,
	};
	let command_without_format = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};
	let error = validate_step_input_overrides(&command_without_format, &step)
		.err()
		.unwrap_or_else(|| panic!("expected missing inherited input error"));
	assert!(
		error
			.to_string()
			.contains("inherits input `format` but the command does not declare it")
	);
}

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn load_workspace_configuration_parses_group_package_max_bump_tables() {
	let root = fixture_path("config/group-package-max-bump");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let group = configuration
		.group_by_id("sdk")
		.unwrap_or_else(|| panic!("expected sdk group"));

	assert_eq!(group.packages, vec!["core", "npm:web"]);
	assert_eq!(
		group.package_max_bumps,
		BTreeMap::from([
			("core".to_string(), BumpSeverity::Minor),
			("npm:web".to_string(), BumpSeverity::None),
		])
	);

	let default_caps_group = configuration
		.group_by_id("default_caps")
		.unwrap_or_else(|| panic!("expected default_caps group"));
	assert_eq!(default_caps_group.packages, vec!["default-a", "default-b"]);
	assert!(default_caps_group.package_max_bumps.is_empty());

	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"default-a",
			root.join("crates/default-a/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"default-b",
			root.join("crates/default-b/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let (version_groups, warnings) = apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));
	assert!(warnings.is_empty());
	let sdk_version_group = version_groups
		.iter()
		.find(|version_group| version_group.group_id == "sdk")
		.unwrap_or_else(|| panic!("expected sdk version group"));
	assert_eq!(
		sdk_version_group.member_max_bumps,
		BTreeMap::from([
			(
				"cargo:crates/core/Cargo.toml".to_string(),
				BumpSeverity::Minor,
			),
			(
				"npm:packages/web/package.json".to_string(),
				BumpSeverity::None
			),
		])
	);
}

#[test]
fn load_workspace_configuration_rejects_invalid_group_changelog_include_members() {
	let root = fixture_path("config/rejects-group-changelog-include-invalid-member");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains(
		"group `sdk` changelog include entry `missing` must reference a package declared in that group"
	));
	assert!(rendered.contains("group changelog include member"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_group_changelog_include_modes() {
	let root = fixture_path("config/rejects-group-changelog-include-invalid-mode");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains(
		"group `sdk` changelog include must be `\"all\"`, `\"group-only\"`, or an array of member package ids"
	));
	assert!(rendered.contains("group changelog include"));
}

#[test]
fn load_workspace_configuration_rejects_empty_group_changelog_include_members() {
	let root = fixture_path("config/rejects-group-changelog-include-empty-member");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains("group `sdk` changelog include entries must not be empty"));
	assert!(rendered.contains("group changelog include member"));
}

#[test]
fn load_workspace_configuration_rejects_group_changelog_tables_without_paths() {
	let root = fixture_path("config/rejects-group-changelog-no-path");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains("group `sdk` changelog must declare a `path`"));
	assert!(rendered.contains("group changelog missing path"));
}

#[test]
fn load_workspace_configuration_requires_package_type_without_default() {
	let root = fixture_path("config/rejects-missing-type");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("must declare `type` or set `[defaults].package_type`"));
	assert!(rendered.contains("single-ecosystem repository"));
}

#[test]
fn load_workspace_configuration_rejects_package_group_namespace_collisions() {
	let root = fixture_path("config/rejects-namespace-collision");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("collides with an existing package or group id"));
	assert!(rendered.contains("package and group ids share one namespace"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_group_members() {
	let root = fixture_path("config/rejects-unknown-group-members");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("references unknown package `missing`"));
	assert!(rendered.contains("declare the package first under [package.<id>]"));
}

#[test]
fn load_workspace_configuration_rejects_multi_group_membership() {
	let root = fixture_path("config/rejects-multi-group-membership");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("error: package `core` belongs to multiple groups"));
	assert!(rendered.contains("--> monochange.toml"));
	assert!(rendered.contains("::: monochange.toml"));
	assert!(
		rendered.contains("= help: move the package into exactly one [group.<id>] declaration")
	);
	assert!(rendered.contains("= note: the first snippet marks the primary failure location"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_versioned_file_dependencies() {
	let root = fixture_path("config/rejects-unknown-versioned-dep");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("unknown versioned file name `missing`"));
	assert!(rendered.contains(
		"reference a declared package id from `versioned_files` or remove the name entry"
	));
}

#[test]
fn load_workspace_configuration_rejects_regex_versioned_files_without_version_capture() {
	let root = fixture_path("config/rejects-regex-versioned-file-without-version-capture");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("must include a named `version` capture"));
}

#[test]
fn load_workspace_configuration_rejects_regex_versioned_files_with_type() {
	let root = fixture_path("config/rejects-regex-versioned-file-with-type");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("regex versioned_files cannot also set `type`"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_regex_versioned_file_patterns() {
	let root = fixture_path("config/rejects-invalid-regex-versioned-file");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("pattern `(` is invalid"));
}

#[test]
fn load_workspace_configuration_rejects_regex_versioned_files_with_prefix() {
	let root = fixture_path("config/rejects-regex-versioned-file-with-prefix");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(
		rendered.contains("regex versioned_files cannot also set `prefix`, `fields`, or `name`")
	);
}

#[test]
fn load_workspace_configuration_rejects_group_string_versioned_files_without_explicit_type() {
	let root = fixture_path("config/rejects-group-string-versioned");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("bare-string `versioned_files`"));
	assert!(rendered.contains("use `versioned_files = [{ path = \"...\", type = \"cargo\" }]`"));
}

#[test]
fn versioned_file_validation_cache_matches_workspace_files_once() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::create_dir_all(root.join("packages/web"))
		.unwrap_or_else(|error| panic!("create package: {error}"));
	fs::create_dir_all(root.join("generated"))
		.unwrap_or_else(|error| panic!("create generated: {error}"));
	fs::write(root.join(".gitignore"), "generated/\n")
		.unwrap_or_else(|error| panic!("write gitignore: {error}"));
	fs::write(
		root.join("packages/web/package.json"),
		"{\"version\":\"1.0.0\"}\n",
	)
	.unwrap_or_else(|error| panic!("write package: {error}"));
	fs::write(root.join("packages/web/README.md"), "# web\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));
	fs::write(root.join("generated/README.md"), "# generated\n")
		.unwrap_or_else(|error| panic!("write generated: {error}"));

	assert_eq!(
		crate::normalize_relative_glob("././packages/**/*.md"),
		"packages/**/*.md"
	);

	let workspace_files = crate::collect_workspace_files(root)
		.unwrap_or_else(|error| panic!("collect workspace files: {error}"));
	assert!(
		workspace_files
			.iter()
			.any(|path| path == Path::new("packages/web/package.json"))
	);
	assert!(
		workspace_files
			.iter()
			.any(|path| path == Path::new("packages/web/README.md"))
	);
	assert!(
		!workspace_files
			.iter()
			.any(|path| path == Path::new("generated/README.md"))
	);

	let mut cache = crate::VersionedFileValidationCache::default();
	assert!(cache.insert_checked_glob("packages/**/*.json".to_string(), "npm"));
	assert!(!cache.insert_checked_glob("packages/**/*.json".to_string(), "npm"));

	let relative_match = cache
		.unsupported_glob_match(root, "././packages/**/*.md", EcosystemType::Npm)
		.unwrap_or_else(|error| panic!("relative glob match: {error}"));
	assert_eq!(
		relative_match.as_deref(),
		Some(root.join("packages/web/README.md").as_path())
	);

	let absolute_glob = root.join("packages/**/*.md").to_string_lossy().to_string();
	let absolute_match = cache
		.unsupported_glob_match(root, &absolute_glob, EcosystemType::Npm)
		.unwrap_or_else(|error| panic!("absolute glob match: {error}"));
	assert_eq!(
		absolute_match.as_deref(),
		Some(root.join("packages/web/README.md").as_path())
	);

	let supported_match = cache
		.unsupported_glob_match(root, "packages/**/*.json", EcosystemType::Npm)
		.unwrap_or_else(|error| panic!("supported glob match: {error}"));
	assert!(supported_match.is_none());
}

fn detailed_lint_rule(options: &[(&str, serde_json::Value)]) -> LintRuleConfig {
	LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: options
			.iter()
			.map(|(key, value)| ((*key).to_string(), value.clone()))
			.collect(),
	}
}

fn changeset_lint_rules(entries: &[(&str, LintRuleConfig)]) -> BTreeMap<String, LintRuleConfig> {
	entries
		.iter()
		.map(|(key, value)| ((*key).to_string(), value.clone()))
		.collect()
}

fn raw_change(bump: Option<BumpSeverity>, change_type: Option<&str>) -> crate::RawChangeEntry {
	crate::RawChangeEntry {
		package: "cargo-core".to_string(),
		bump,
		version: None,
		reason: None,
		details: None,
		change_type: change_type.map(str::to_string),
		caused_by: Vec::new(),
	}
}

fn expect_config_error(result: MonochangeResult<()>, expected: &str) {
	let error = result.expect_err("expected config error").to_string();
	assert!(error.contains(expected), "unexpected error: {error}");
}

#[test]
fn changeset_lint_rule_parsing_covers_enabled_disabled_and_invalid_options() {
	let settings = crate::changeset_lint_settings_from_rules(&changeset_lint_rules(&[
		(
			"packages/name",
			LintRuleConfig::Severity(LintSeverity::Error),
		),
		(
			"changesets/duplicate",
			LintRuleConfig::Severity(LintSeverity::Off),
		),
		(
			"changesets/no_section_headings",
			LintRuleConfig::Severity(LintSeverity::Error),
		),
		(
			"changesets/summary",
			detailed_lint_rule(&[
				("required", serde_json::json!(true)),
				("heading_level", serde_json::json!(2)),
				("min_length", serde_json::json!(3)),
				("max_length", serde_json::json!(80)),
				("forbid_trailing_period", serde_json::json!(true)),
				("forbid_conventional_commit_prefix", serde_json::json!(true)),
			]),
		),
		(
			"changesets/bump/minor",
			detailed_lint_rule(&[
				("required_sections", serde_json::json!(["Impact"])),
				("min_body_chars", serde_json::json!(12)),
				("max_body_chars", serde_json::json!(200)),
				("require_code_block", serde_json::json!(true)),
				("required_bump", serde_json::json!("minor")),
				("forbidden_headings", serde_json::json!(["Minor"])),
			]),
		),
		(
			"changesets/types/Feature",
			detailed_lint_rule(&[("required_sections", serde_json::json!(["Usage"]))]),
		),
		(
			"changesets/unknown",
			LintRuleConfig::Severity(LintSeverity::Error),
		),
	]))
	.unwrap_or_else(|error| panic!("parse changeset lints: {error}"));

	assert!(settings.no_section_headings);
	assert!(settings.summary.required);
	assert_eq!(settings.summary.heading_level, Some(2));
	assert!(settings.bump.contains_key(&BumpSeverity::Minor));
	assert!(settings.types.contains_key("Feature"));

	for (rule_id, config, expected) in [
		(
			"changesets/bump/mega",
			LintRuleConfig::Severity(LintSeverity::Error),
			"uses an unknown bump severity",
		),
		(
			"changesets/types/",
			LintRuleConfig::Severity(LintSeverity::Error),
			"must include a type name",
		),
		(
			"changesets/summary",
			detailed_lint_rule(&[("required", serde_json::json!("yes"))]),
			"required must be a boolean",
		),
		(
			"changesets/summary",
			detailed_lint_rule(&[(
				"forbid_conventional_commit_prefix",
				serde_json::json!("yes"),
			)]),
			"forbid_conventional_commit_prefix must be a boolean",
		),
		(
			"changesets/summary",
			detailed_lint_rule(&[("heading_level", serde_json::json!("2"))]),
			"heading_level must be a non-negative integer",
		),
		(
			"changesets/bump/major",
			detailed_lint_rule(&[("required_sections", serde_json::json!("Impact"))]),
			"required_sections must be a string array",
		),
		(
			"changesets/bump/major",
			detailed_lint_rule(&[("required_sections", serde_json::json!([1]))]),
			"required_sections must be a string array",
		),
		(
			"changesets/bump/major",
			detailed_lint_rule(&[("required_bump", serde_json::json!(1))]),
			"required_bump must be a bump severity string",
		),
		(
			"changesets/bump/major",
			detailed_lint_rule(&[("required_bump", serde_json::json!("mega"))]),
			"required_bump uses an unknown bump severity",
		),
	] {
		let error =
			crate::changeset_lint_settings_from_rules(&changeset_lint_rules(&[(rule_id, config)]))
				.expect_err("invalid lint rule should be rejected")
				.to_string();
		assert!(error.contains(expected), "unexpected error: {error}");
	}
}

#[test]
fn changeset_lint_validation_covers_summary_and_scoped_rule_errors() {
	let changelog = ChangelogSettings::default();
	let mut valid = ChangesetLintSettings::default();
	valid.bump.insert(
		BumpSeverity::Minor,
		ChangesetScopedLintSettings {
			required_sections: vec!["Impact".to_string()],
			min_body_chars: Some(5),
			max_body_chars: Some(200),
			require_code_block: false,
			required_bump: None,
			forbidden_headings: Vec::new(),
		},
	);
	valid.types.insert(
		"feat".to_string(),
		ChangesetScopedLintSettings {
			required_sections: vec!["Usage".to_string()],
			min_body_chars: None,
			max_body_chars: None,
			require_code_block: false,
			required_bump: Some(BumpSeverity::Minor),
			forbidden_headings: vec!["Feature".to_string()],
		},
	);
	crate::validate_changeset_lint_settings(&valid, &changelog)
		.unwrap_or_else(|error| panic!("valid lint settings: {error}"));

	let invalid_cases = [
		(
			ChangesetLintSettings {
				summary: ChangesetSummaryLintSettings {
					heading_level: Some(7),
					..ChangesetSummaryLintSettings::default()
				},
				..ChangesetLintSettings::default()
			},
			"heading_level must be between 1 and 6",
		),
		(
			ChangesetLintSettings {
				summary: ChangesetSummaryLintSettings {
					min_length: Some(20),
					max_length: Some(10),
					..ChangesetSummaryLintSettings::default()
				},
				..ChangesetLintSettings::default()
			},
			"min_length must not exceed max_length",
		),
		(
			ChangesetLintSettings {
				summary: ChangesetSummaryLintSettings {
					min_length: Some(80),
					max_heading_length: Some(60),
					..ChangesetSummaryLintSettings::default()
				},
				..ChangesetLintSettings::default()
			},
			"min_length must not exceed max_heading_length",
		),
	];

	for (settings, expected) in invalid_cases {
		let error = crate::validate_changeset_lint_settings(&settings, &changelog)
			.expect_err("invalid lint settings should be rejected")
			.to_string();
		assert!(error.contains(expected), "unexpected error: {error}");
	}

	for (scoped, expected) in [
		(
			ChangesetScopedLintSettings {
				min_body_chars: Some(20),
				max_body_chars: Some(10),
				..ChangesetScopedLintSettings::default()
			},
			"min_body_chars must not exceed max_body_chars",
		),
		(
			ChangesetScopedLintSettings {
				required_sections: vec![" ".to_string()],
				..ChangesetScopedLintSettings::default()
			},
			"required_sections must not include empty values",
		),
		(
			ChangesetScopedLintSettings {
				forbidden_headings: vec![String::new()],
				..ChangesetScopedLintSettings::default()
			},
			"forbidden_headings must not include empty values",
		),
	] {
		let mut settings = ChangesetLintSettings::default();
		settings.bump.insert(BumpSeverity::Patch, scoped);
		let error = crate::validate_changeset_lint_settings(&settings, &changelog)
			.expect_err("invalid scoped lint settings should be rejected")
			.to_string();
		assert!(error.contains(expected), "unexpected error: {error}");
	}

	let mut invalid_type_settings = ChangesetLintSettings::default();
	invalid_type_settings.types.insert(
		"feat".to_string(),
		ChangesetScopedLintSettings {
			required_sections: vec![String::new()],
			..ChangesetScopedLintSettings::default()
		},
	);
	let error = crate::validate_changeset_lint_settings(&invalid_type_settings, &changelog)
		.expect_err("invalid type scoped lint settings should be rejected")
		.to_string();
	assert!(
		error.contains("changesets/types/feat.required_sections must not include empty values"),
		"unexpected error: {error}"
	);
}

#[test]
fn lint_markdown_changeset_covers_summary_type_and_scope_failures() {
	let path = Path::new("change.md");
	let changes = [raw_change(Some(BumpSeverity::Minor), Some("Feature"))];
	let mut settings = ChangesetLintSettings::default();
	assert!(crate::lint_markdown_changeset("", &changes, &settings, path).is_ok());

	settings.summary.required = true;
	expect_config_error(
		crate::lint_markdown_changeset("", &changes, &settings, path),
		"changeset body must start with a summary heading",
	);
	expect_config_error(
		crate::lint_markdown_changeset("plain summary", &changes, &settings, path),
		"changeset body must start with a summary heading",
	);

	settings.summary.heading_level = Some(2);
	expect_config_error(
		crate::lint_markdown_changeset("# Wrong level", &changes, &settings, path),
		"summary heading must use level 2",
	);

	settings.summary.min_length = Some(20);
	expect_config_error(
		crate::lint_markdown_changeset("## Short", &changes, &settings, path),
		"summary must be at least 20 characters",
	);
	settings.summary.min_length = None;
	settings.summary.max_length = Some(5);
	expect_config_error(
		crate::lint_markdown_changeset("## Summary too long", &changes, &settings, path),
		"header must be at most 5 characters",
	);
	settings.summary.required = false;
	expect_config_error(
		crate::lint_markdown_changeset("summary too long", &changes, &settings, path),
		"summary must be at most 5 characters",
	);
	settings.summary.required = true;

	settings.summary.max_length = None;
	settings.summary.forbid_trailing_period = true;
	expect_config_error(
		crate::lint_markdown_changeset("## Summary.", &changes, &settings, path),
		"summary must not end with a period",
	);
	settings.summary.forbid_trailing_period = false;
	settings.summary.forbid_conventional_commit_prefix = true;
	expect_config_error(
		crate::lint_markdown_changeset("## feat(core): add behavior", &changes, &settings, path),
		"summary must not use a conventional-commit prefix",
	);
	assert!(!crate::has_conventional_commit_prefix("Add behavior"));

	settings.summary = ChangesetSummaryLintSettings::default();
	settings.no_section_headings = true;
	expect_config_error(
		crate::lint_markdown_changeset("# Summary\n\n## Feature", &changes, &settings, path),
		"must not also be used as a heading",
	);
	assert!(
		crate::lint_markdown_changeset("# Summary\n\n## Details", &changes, &settings, path)
			.is_ok()
	);

	settings.no_section_headings = false;
	settings.types.insert(
		"feature".to_string(),
		ChangesetScopedLintSettings {
			required_sections: vec!["Impact".to_string()],
			..ChangesetScopedLintSettings::default()
		},
	);
	expect_config_error(
		crate::lint_markdown_changeset("# Summary", &changes, &settings, path),
		"changeset must include a `Impact` section",
	);
	assert!(
		crate::lint_markdown_changeset("# Summary\n\n## Impact", &changes, &settings, path).is_ok()
	);
}

#[test]
fn lint_markdown_scope_covers_each_configured_requirement() {
	let path = Path::new("change.md");
	let body = "# Summary\n\n## Impact\n\n```rust\nlet value = 1;\n```";
	let change = raw_change(Some(BumpSeverity::Patch), Some("breaking"));

	expect_config_error(
		crate::lint_markdown_scope(
			body,
			&change,
			&ChangesetScopedLintSettings {
				required_bump: Some(BumpSeverity::Major),
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"requires bump `major`, found `patch`",
	);
	expect_config_error(
		crate::lint_markdown_scope(
			body,
			&raw_change(None, Some("breaking")),
			&ChangesetScopedLintSettings {
				required_bump: Some(BumpSeverity::Major),
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"requires bump `major`, found `auto`",
	);
	expect_config_error(
		crate::lint_markdown_scope(
			"# Summary",
			&change,
			&ChangesetScopedLintSettings {
				required_sections: vec!["Impact".to_string()],
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"changeset must include a `Impact` section",
	);
	expect_config_error(
		crate::lint_markdown_scope(
			body,
			&change,
			&ChangesetScopedLintSettings {
				forbidden_headings: vec!["Impact".to_string()],
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"changeset must not use `Impact` as a heading",
	);
	assert!(
		crate::lint_markdown_scope(
			body,
			&change,
			&ChangesetScopedLintSettings {
				forbidden_headings: vec!["Missing".to_string()],
				..ChangesetScopedLintSettings::default()
			},
			path,
		)
		.is_ok()
	);
	expect_config_error(
		crate::lint_markdown_scope(
			"# Tiny",
			&change,
			&ChangesetScopedLintSettings {
				min_body_chars: Some(20),
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"body must be at least 20 characters",
	);
	expect_config_error(
		crate::lint_markdown_scope(
			body,
			&change,
			&ChangesetScopedLintSettings {
				max_body_chars: Some(10),
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"body must be at most 10 characters",
	);
	expect_config_error(
		crate::lint_markdown_scope(
			"# Summary\n\n## Impact",
			&change,
			&ChangesetScopedLintSettings {
				require_code_block: true,
				..ChangesetScopedLintSettings::default()
			},
			path,
		),
		"must include a fenced code block",
	);

	assert!(
		crate::lint_markdown_scope(
			"# Summary\n\n~~~text\ncode\n~~~",
			&change,
			&ChangesetScopedLintSettings {
				require_code_block: true,
				..ChangesetScopedLintSettings::default()
			},
			path,
		)
		.is_ok()
	);
}

#[test]
fn changelog_stream_validation_rejects_ambiguous_or_invalid_outputs() {
	let package = package_definition("app", "crates/app");
	let mut settings = raw_changelog_settings();
	settings.sections.insert(
		"features".to_string(),
		monochange_core::ChangelogSectionDef {
			heading: "Features".to_string(),
			description: None,
			priority: 10,
		},
	);
	settings.types.insert(
		"app_feature".to_string(),
		crate::RawChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
			stream: "missing".to_string(),
		},
	);
	expect_config_error(
		crate::validate_changelog_configuration(
			"[changelog.types.app_feature]\nbump = \"minor\"\n",
			&settings,
			std::slice::from_ref(&package),
			&[],
		),
		"references stream `missing`",
	);

	let invalid_outputs = [
		(
			"default",
			changelog_output("default", "notes.md", vec!["app"]),
			"outputs.default is reserved",
		),
		(
			"user",
			changelog_output("missing", "notes.md", vec!["app"]),
			"references stream `missing`",
		),
		(
			"user",
			changelog_output("default", " ", vec!["app"]),
			"path must not be empty",
		),
		(
			"user",
			changelog_output("default", "{{ audience }}/notes.md", vec!["app"]),
			"path uses unsupported variables: audience",
		),
		(
			"user",
			changelog_output("default", "notes.md", Vec::new()),
			"targets must include at least one",
		),
		(
			"user",
			changelog_output("default", "notes.md", vec!["app", "app"]),
			"contains duplicate target `app`",
		),
		(
			"user",
			changelog_output("default", "notes.md", vec!["missing"]),
			"references unknown package or group `missing`",
		),
	];

	for (output_id, output, expected) in invalid_outputs {
		let mut settings = raw_changelog_settings();
		settings.outputs.insert(output_id.to_string(), output);
		expect_config_error(
			crate::validate_changelog_configuration(
				"",
				&settings,
				std::slice::from_ref(&package),
				&[],
			),
			expected,
		);
	}

	for format in [ChangelogFormat::Json, ChangelogFormat::Text] {
		let mut settings = raw_changelog_settings();
		let mut output = changelog_output("default", "notes", vec!["app"]);
		output.format = format;
		settings.outputs.insert("user".to_string(), output);
		expect_config_error(
			crate::validate_changelog_configuration(
				"",
				&settings,
				std::slice::from_ref(&package),
				&[],
			),
			"must use `mode = \"release\"`",
		);
	}

	let mut settings = raw_changelog_settings();
	let mut output = changelog_output("default", "notes.md", vec!["app"]);
	output.mode = monochange_core::ChangelogOutputMode::Release;
	output.initial_header = Some("# Notes".to_string());
	settings.outputs.insert("user".to_string(), output);
	expect_config_error(
		crate::validate_changelog_configuration("", &settings, &[package], &[]),
		"initial_header is only supported with `mode = \"append\"`",
	);
}

#[test]
fn changelog_output_and_provider_validation_accepts_only_compatible_destinations() {
	assert!(crate::validate_append_changelog_format("disabled", None).is_ok());
	let markdown = ChangelogTarget {
		path: PathBuf::from("CHANGELOG.md"),
		format: ChangelogFormat::Monochange,
		initial_header: None,
	};
	assert!(crate::validate_append_changelog_format("package `app`", Some(&markdown)).is_ok());

	for format in [ChangelogFormat::Json, ChangelogFormat::Text] {
		let standalone = ChangelogTarget {
			format,
			..markdown.clone()
		};
		expect_config_error(
			crate::validate_append_changelog_format("package `app`", Some(&standalone)),
			"configure JSON or text under [changelog.outputs]",
		);
	}

	let mut package = package_definition("app", "crates/app");
	package.changelog = Some(ChangelogTarget {
		format: ChangelogFormat::Json,
		..markdown.clone()
	});
	let mut group = GroupDefinition {
		id: "apps".to_string(),
		packages: vec!["app".to_string()],
		package_max_bumps: BTreeMap::new(),
		bump_propagation: None,
		changelog: Some(ChangelogTarget {
			format: ChangelogFormat::Text,
			..markdown.clone()
		}),
		changelog_include: GroupChangelogInclude::All,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		version_source: VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
	};
	expect_config_error(
		crate::validate_changelog_configuration(
			"",
			&raw_changelog_settings(),
			std::slice::from_ref(&package),
			std::slice::from_ref(&group),
		),
		"package `app` changelog uses standalone format `json`",
	);
	package.changelog = None;
	expect_config_error(
		crate::validate_changelog_configuration(
			"",
			&raw_changelog_settings(),
			std::slice::from_ref(&package),
			std::slice::from_ref(&group),
		),
		"group `apps` changelog uses standalone format `text`",
	);
	group.changelog = None;

	let mut changelog = ChangelogSettings::default();
	changelog.outputs.insert(
		"user".to_string(),
		changelog_output("default", "notes.md", vec!["app"]),
	);
	assert!(crate::validate_source_changelog_output(None, &changelog).is_ok());
	let mut source = sample_source_configuration(SourceProvider::GitHub);
	assert!(crate::validate_source_changelog_output(Some(&source), &changelog).is_ok());
	source.releases.changelog_output = "user".to_string();
	assert!(crate::validate_source_changelog_output(Some(&source), &changelog).is_ok());
	source.releases.changelog_output = "missing".to_string();

	expect_config_error(
		crate::validate_source_changelog_output(Some(&source), &changelog),
		"references unknown [changelog.outputs] entry `missing`",
	);
}

#[test]
fn load_change_signals_applies_toml_type_defaults_and_validates_context_types() {
	let tempdir = setup_fixture("changeset-target-metadata/render-workspace");
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			tempdir.path().join("crates/core/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"app",
			tempdir.path().join("crates/app/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let type_path = tempdir.path().join("type-only.toml");
	fs::write(
		&type_path,
		r#"[[changes]]
package = "sdk"
type = "test"
reason = "Exercise a group TOML type default."
"#,
	)
	.unwrap_or_else(|error| panic!("write type-only.toml: {error}"));
	let loaded = load_changeset_file(&type_path, &configuration, &packages)
		.unwrap_or_else(|error| panic!("typed changeset: {error}"));
	let target = loaded
		.targets
		.iter()
		.find(|target| target.id == "sdk")
		.unwrap_or_else(|| panic!("expected sdk target"));
	assert_eq!(target.bump, Some(BumpSeverity::Minor));
	assert_eq!(target.change_type.as_deref(), Some("test"));
	assert_eq!(loaded.signals.len(), 2);
	assert!(loaded.signals.iter().all(|signal| {
		signal.requested_bump == Some(BumpSeverity::Minor)
			&& signal.change_type.as_deref() == Some("test")
	}));

	let version_path = tempdir.path().join("version-only.toml");
	fs::write(
		&version_path,
		r#"[[changes]]
package = "sdk"
version = "2.0.0"
reason = "Exercise group version inference."
"#,
	)
	.unwrap_or_else(|error| panic!("write version-only.toml: {error}"));
	let loaded = load_changeset_file(&version_path, &configuration, &packages)
		.unwrap_or_else(|error| panic!("versioned changeset: {error}"));
	let target = loaded
		.targets
		.iter()
		.find(|target| target.id == "sdk")
		.unwrap_or_else(|| panic!("expected sdk target"));
	assert_eq!(target.bump, Some(BumpSeverity::Major));
	assert!(
		loaded
			.signals
			.iter()
			.all(|signal| signal.requested_bump == Some(BumpSeverity::Major))
	);

	let packages_without_app = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		tempdir.path().join("crates/core/Cargo.toml"),
		tempdir.path().to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let missing_member_error =
		load_change_signals(&version_path, &configuration, &packages_without_app)
			.err()
			.unwrap_or_else(|| panic!("expected missing group member error"));
	assert!(
		missing_member_error
			.to_string()
			.contains("change package reference `app` did not match any discovered package")
	);

	let invalid_group_path = tempdir.path().join("invalid-group-type.toml");
	fs::write(
		&invalid_group_path,
		r#"[[changes]]
package = "sdk"
type = "nope"
"#,
	)
	.unwrap_or_else(|error| panic!("write invalid-group-type.toml: {error}"));
	let group_error = load_change_signals(&invalid_group_path, &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected invalid group type error"));
	assert!(
		group_error
			.to_string()
			.contains("target `sdk` has invalid type `nope`")
	);

	let invalid_package_path = tempdir.path().join("invalid-package-type.toml");
	fs::write(
		&invalid_package_path,
		r#"[[changes]]
package = "core"
type = "nope"
"#,
	)
	.unwrap_or_else(|error| panic!("write invalid-package-type.toml: {error}"));
	let package_error = load_change_signals(&invalid_package_path, &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected invalid package type error"));
	assert!(
		package_error
			.to_string()
			.contains("target `core` has invalid type `nope`")
	);
}

#[test]
fn load_change_signals_rejects_duplicate_package_entries() {
	let root = fixture_path("config/rejects-change-duplicate-entries");
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let rendered = load_change_signals(&root.join("change.toml"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected duplicate entry error"))
		.render();
	assert!(rendered.contains("duplicate change entry"));
}

#[test]
fn normalize_publish_settings_rejects_conflicting_placeholder_sources() {
	let error = crate::normalize_publish_settings(
		r"
[package.core.publish.placeholder]
",
		Some(&monochange_core::PublishSettings {
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			placeholder: monochange_core::PlaceholderSettings {
				readme: Some("inline".to_string()),
				readme_file: Some(PathBuf::from("docs/placeholder.md")),
			},
			..monochange_core::PublishSettings::default()
		}),
		crate::RawPublishSettings::default(),
		"package",
		"core",
		EcosystemType::Cargo,
	)
	.err()
	.unwrap_or_else(|| panic!("expected placeholder conflict error"));
	assert!(
		error.to_string().contains(
			"package `core` publish.placeholder cannot set both `readme` and `readme_file`"
		)
	);
}

#[test]
fn load_workspace_configuration_rejects_staged_publish_flow_for_non_npm_ecosystem() {
	let root = fixture_path("config/rejects-publish-flow-non-npm");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected staged publish flow error"));
	assert!(error.to_string().contains(
		"ecosystems `python` uses staged publishing for an ecosystem without staged publish support"
	));
	assert!(error.to_string().contains("flow = \"staged\""));
}

#[test]
fn load_workspace_configuration_rejects_builtin_publish_registry_override() {
	let root = fixture_path("config/rejects-publish-builtin-registry-override");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected builtin publish registry error"));
	assert!(
		error.to_string().contains(
			"package `core` uses built-in publishing with an unsupported registry override"
		)
	);
	assert!(error.to_string().contains("mode = \"external\""));
}

#[test]
fn load_workspace_configuration_rejects_unknown_input_on_validate_step() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-validate");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error.to_string().contains("unknown input override `mode`"),
		"error was: {error}"
	);
	assert!(
		error.to_string().contains("valid inputs: format"),
		"error was: {error}"
	);
}

#[test]
fn load_change_signals_reports_pretty_frontmatter_parse_errors_with_fix_hint() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.\"@monochange/skill\"]\npath = \"crates/core\"\ntype = \"cargo\"\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir crate: {error}"));
	fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo manifest: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let change_path = tempdir.path().join("change.md");
	fs::write(
		&change_path,
		"---\n@monochange/skill: patch\n---\n\n# broken\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));

	let error = load_change_signals(&change_path, &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(
		rendered.contains("error: failed to parse"),
		"rendered: {rendered}"
	);
	assert!(
		rendered.contains(&format!("--> {}:2:1", change_path.display())),
		"rendered: {rendered}"
	);
	assert!(
		rendered.contains("2 | @monochange/skill: patch"),
		"rendered: {rendered}"
	);
	assert!(
		rendered.contains("wrap package or group ids that contain characters like `@`, `/`, `:`, or spaces in double quotes"),
		"rendered: {rendered}"
	);
}

#[test]
fn source_diagnostic_helpers_cover_empty_labels_sorting_and_fallback_spans() {
	let source = "alpha\nbeta\ngamma";
	let empty = render_source_diagnostic("demo.md", source, "plain failure", &[], None);
	assert!(empty.contains("error: plain failure"), "{empty}");
	assert!(empty.contains("  --> demo.md:1:1"), "{empty}");
	assert!(!empty.contains("  = help:"), "{empty}");
	assert!(!empty.contains("  = note:"), "{empty}");

	let labels = vec![
		LabeledSpan::new_with_span(Some("primary".to_string()), range_to_span(6..10)),
		LabeledSpan::new_with_span(Some("later".to_string()), range_to_span(11..16)),
		LabeledSpan::new_with_span(Some("earlier".to_string()), range_to_span(0..0)),
	];
	let sorted = sort_labels_by_location(&labels);
	assert_eq!(sorted.first().and_then(LabeledSpan::label), Some("primary"));
	assert_eq!(sorted.get(1).and_then(LabeledSpan::label), Some("earlier"));
	assert_eq!(sorted.get(2).and_then(LabeledSpan::label), Some("later"));

	let rendered = render_source_diagnostic(
		"demo.md",
		source,
		"annotated failure",
		&labels,
		Some("try fixing it"),
	);
	assert!(rendered.contains("  --> demo.md:2:1"), "{rendered}");
	assert!(rendered.contains("  ::: demo.md:1:1"), "{rendered}");
	assert!(rendered.contains("  ::: demo.md:3:1"), "{rendered}");
	assert!(rendered.contains("^ primary"), "{rendered}");
	assert!(rendered.contains("^ earlier"), "{rendered}");
	assert!(rendered.contains("^ later"), "{rendered}");
	assert!(rendered.contains("  = help: try fixing it"), "{rendered}");

	assert!(
		rendered.contains("  = note: the first snippet marks the primary failure location"),
		"{rendered}"
	);

	let secondary = render_source_snippet(
		"demo.md",
		source,
		&LabeledSpan::new_with_span(None, range_to_span(0..0)),
		false,
	);
	assert_eq!(secondary.first(), Some(&"  ::: demo.md:1:1".to_string()));
	assert!(
		secondary.iter().any(|line| line.contains("^ here")),
		"{secondary:?}"
	);

	assert!(render_source_snippets("demo.md", source, &[]).is_empty());
	assert!(sort_labels_by_location(&[]).is_empty());
	assert!(render_diagnostic_notes(&[]).is_empty());
	let single_label = labels
		.first()
		.cloned()
		.map(|label| vec![label])
		.unwrap_or_default();
	assert!(render_diagnostic_notes(&single_label).is_empty());
	assert_eq!(line_index_for_offset(source, usize::MAX), 2);
	assert_eq!(line_and_column_for_offset(source, usize::MAX), (3, 6));
	assert_eq!(frontmatter_span_for_line_column(source, 2, 99), 10..11);

	assert_eq!(
		frontmatter_span_for_line_column(source, 99, 1),
		source.len()..source.len()
	);
}

fn package_definition(id: &str, path: &str) -> monochange_core::PackageDefinition {
	monochange_core::PackageDefinition {
		id: id.to_string(),
		path: PathBuf::from(path),
		package_type: PackageType::Cargo,
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
		version_format: VersionFormat::Namespaced,
		version_source: VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
		publish: monochange_core::PublishSettings::default(),
		cli: None,

		values: BTreeMap::new(),
		display_version: None,
	}
}

fn raw_changelog_settings() -> crate::RawChangelogSettings {
	crate::RawChangelogSettings {
		templates: Vec::new(),
		sections: BTreeMap::new(),
		section_thresholds: monochange_core::ChangelogSectionThresholds::default(),
		types: BTreeMap::new(),
		streams: BTreeMap::new(),
		outputs: BTreeMap::new(),
		style: monochange_core::ChangelogStyle::default(),
		release_notes: monochange_core::ReleaseNotesStyleOverrides::default(),
	}
}

fn changelog_output(
	stream: &str,
	path: &str,
	targets: Vec<&str>,
) -> monochange_core::ChangelogOutputDefinition {
	monochange_core::ChangelogOutputDefinition {
		stream: stream.to_string(),
		path: path.to_string(),
		format: ChangelogFormat::Monochange,
		mode: monochange_core::ChangelogOutputMode::Append,
		targets: targets.into_iter().map(str::to_string).collect(),
		initial_header: None,
	}
}

fn cli_input(name: &str, kind: CliInputKind) -> CliInputDefinition {
	CliInputDefinition {
		name: name.to_string(),
		kind,
		help_text: None,
		required: false,
		default: None,
		choices: Vec::new(),
		short: None,
	}
}

fn cli_command(name: &str, steps: Vec<CliStepDefinition>) -> CliCommandDefinition {
	CliCommandDefinition {
		name: name.to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps,
		dry_run: false,
	}
}

fn sample_source_configuration(provider: SourceProvider) -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings {
			enabled: true,
			..Default::default()
		},
		pull_requests: ProviderMergeRequestSettings {
			enabled: true,
			..Default::default()
		},
	}
}

#[test]
fn load_changeset_file_reports_io_and_toml_parse_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let missing = load_changeset_file(&tempdir.path().join("missing.toml"), &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected missing file error"));
	assert!(missing.to_string().contains("failed to read"));

	let invalid = tempdir.path().join("invalid.toml");
	fs::write(&invalid, "changes = [").unwrap_or_else(|error| panic!("write invalid: {error}"));
	let parse_error = load_changeset_file(&invalid, &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(parse_error.to_string().contains("failed to parse"));
}

#[test]
fn validate_source_and_changeset_settings_reject_empty_values() {
	let source_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: " ".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: ProviderMergeRequestSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected source validation error"));
	assert!(
		source_error
			.to_string()
			.contains("[source].owner must not be empty")
	);

	let changeset_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings {
			affected: monochange_core::ChangesetAffectedSettings {
				skip_labels: vec![String::new()],
				..Default::default()
			},
			..Default::default()
		},
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected changeset validation error"));
	assert!(
		changeset_error
			.to_string()
			.contains("[changesets.affected].skip_labels must not include empty values")
	);

	let classification_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings {
			classification: monochange_core::ChangesetClassificationSettings {
				skip_labels: vec![" ".to_string()],
			},
			..Default::default()
		},
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected classification validation error"));
	assert!(
		classification_error
			.to_string()
			.contains("[changesets.classification].skip_labels must not include empty values")
	);

	let affected_empty_path_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings {
			affected: monochange_core::ChangesetAffectedSettings {
				changed_paths: vec![" ".to_string()],
				..Default::default()
			},
			..Default::default()
		},
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected affected path validation error"));
	assert!(
		affected_empty_path_error
			.to_string()
			.contains("[changesets.affected].changed_paths must not include empty values")
	);

	let source_repo_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: "ifiokjr".to_string(),
			repo: " ".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: ProviderMergeRequestSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected source repo validation error"));
	assert!(
		source_repo_error
			.to_string()
			.contains("[source].repo must not be empty")
	);
}

#[test]
fn validate_cli_rejects_invalid_command_shapes() {
	let duplicate = crate::validate_cli(&[
		cli_command(
			"release",
			vec![CliStepDefinition::Validate {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		),
		cli_command(
			"release",
			vec![CliStepDefinition::Validate {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		),
	])
	.err()
	.unwrap_or_else(|| panic!("expected duplicate command error"));
	assert!(
		duplicate
			.to_string()
			.contains("duplicate CLI command `release`")
	);

	let reserved = crate::validate_cli(&[cli_command(
		"help",
		vec![CliStepDefinition::Validate {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected reserved name error"));
	assert!(reserved.to_string().contains("reserved built-in command"));

	let no_steps = crate::validate_cli(&[cli_command("release", Vec::new())])
		.err()
		.unwrap_or_else(|| panic!("expected missing steps error"));
	assert!(
		no_steps
			.to_string()
			.contains("must define at least one step")
	);
}

#[test]
fn validate_cli_rejects_invalid_inputs_and_step_metadata() {
	let mut duplicate_inputs = cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
	);
	duplicate_inputs.inputs = vec![
		cli_input("token", CliInputKind::String),
		cli_input("token", CliInputKind::String),
	];
	let duplicate_input_error = crate::validate_cli(&[duplicate_inputs])
		.err()
		.unwrap_or_else(|| panic!("expected duplicate input error"));
	assert!(
		duplicate_input_error
			.to_string()
			.contains("defines duplicate input `token`")
	);

	let mut invalid_choice_default = cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
	);
	let mut channel = cli_input("channel", CliInputKind::Choice);
	channel.choices = vec!["stable".to_string()];
	channel.default = Some("beta".to_string());
	invalid_choice_default.inputs = vec![channel];
	let choice_error = crate::validate_cli(&[invalid_choice_default])
		.err()
		.unwrap_or_else(|| panic!("expected invalid choice default"));
	assert!(
		choice_error
			.to_string()
			.contains("default `beta` is not one of the configured choices")
	);

	let mut invalid_boolean_default = cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
	);
	let mut confirm = cli_input("confirm", CliInputKind::Boolean);
	confirm.default = Some("yes".to_string());
	invalid_boolean_default.inputs = vec![confirm];
	let boolean_error = crate::validate_cli(&[invalid_boolean_default])
		.err()
		.unwrap_or_else(|| panic!("expected invalid boolean default"));
	assert!(
		boolean_error
			.to_string()
			.contains("boolean default must be `true` or `false`")
	);

	let empty_step_name = crate::validate_cli(&[cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("   ".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected empty step name error"));
	assert!(empty_step_name.to_string().contains("has an empty `name`"));

	let duplicate_step_names = crate::validate_cli(&[cli_command(
		"release",
		vec![
			CliStepDefinition::Validate {
				name: Some("Shared".to_string()),
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			},
			CliStepDefinition::Discover {
				name: Some("Shared".to_string()),
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			},
		],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected duplicate step name error"));
	assert!(
		duplicate_step_names
			.to_string()
			.contains("duplicate step name `Shared`")
	);

	let invalid_command = crate::validate_cli(&[cli_command(
		"release",
		vec![CliStepDefinition::Command {
			name: Some("Run command".to_string()),
			when: Some(" ".to_string()),
			always_run: false,
			show_progress: None,
			command: String::new(),
			dry_run_command: Some(" ".to_string()),
			shell: ShellConfig::Default,
			id: Some(" ".to_string()),
			variables: None,
			inputs: BTreeMap::from([(
				String::new(),
				CliStepInputValue::String("value".to_string()),
			)]),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected invalid command step"));
	assert!(
		invalid_command
			.to_string()
			.contains("has an empty `when` condition")
	);

	let empty_command = crate::validate_cli(&[cli_command(
		"release",
		vec![CliStepDefinition::Command {
			name: Some("Run command".to_string()),
			when: None,
			always_run: false,
			show_progress: None,
			command: " ".to_string(),
			dry_run_command: None,
			shell: ShellConfig::Default,
			id: Some("run-command".to_string()),
			variables: None,
			inputs: BTreeMap::new(),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected empty command error"));
	assert!(
		empty_command
			.to_string()
			.contains("command steps must provide a non-empty command")
	);

	let empty_dry_run_command = crate::validate_cli(&[cli_command(
		"release",
		vec![CliStepDefinition::Command {
			name: Some("Run command".to_string()),
			when: None,
			always_run: false,
			show_progress: None,
			command: "echo release".to_string(),
			dry_run_command: Some(" ".to_string()),
			shell: ShellConfig::Default,
			id: Some("run-command".to_string()),
			variables: None,
			inputs: BTreeMap::new(),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected empty dry-run command error"));
	assert!(
		empty_dry_run_command
			.to_string()
			.contains("`dry_run_command` must provide a non-empty command")
	);

	let empty_input_name = crate::validate_cli(&[CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![cli_input(" ", CliInputKind::String)],
		steps: vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	}])
	.err()
	.unwrap_or_else(|| panic!("expected empty input name error"));
	assert!(
		empty_input_name
			.to_string()
			.contains("has an input with an empty name")
	);

	let reserved_input_name = crate::validate_cli(&[CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![cli_input("help", CliInputKind::String)],
		steps: vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	}])
	.err()
	.unwrap_or_else(|| panic!("expected reserved input name error"));
	assert!(
		reserved_input_name
			.to_string()
			.contains("collides with an implicit command flag")
	);

	let empty_choices = crate::validate_cli(&[CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![cli_input("channel", CliInputKind::Choice)],
		steps: vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	}])
	.err()
	.unwrap_or_else(|| panic!("expected empty choices error"));
	assert!(
		empty_choices
			.to_string()
			.contains("must define at least one choice")
	);
}

#[test]
fn validate_cli_runtime_requirements_enforce_source_features() {
	let publish_without_source = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		None,
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing source error"));
	assert!(
		publish_without_source
			.to_string()
			.contains("PublishRelease")
	);
	assert!(
		publish_without_source
			.to_string()
			.contains("not configured")
	);

	let mut source = sample_source_configuration(SourceProvider::GitHub);
	source.releases.enabled = false;
	let publish_disabled = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&source),
	)
	.err()
	.unwrap_or_else(|| panic!("expected disabled release error"));
	assert!(publish_disabled.to_string().contains("PublishRelease"));
	assert!(publish_disabled.to_string().contains("enabled` is false"));

	let mut pull_requests_disabled = sample_source_configuration(SourceProvider::GitHub);
	pull_requests_disabled.pull_requests.enabled = false;
	let open_request_error = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				always_run: false,
				no_verify: false,
				stage_all: false,
				backend: monochange_core::ReleaseBackend::default(),
				hosted_auth: monochange_core::HostedCommitAuth::default(),
				hosted_url: None,
				inputs: BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&pull_requests_disabled),
	)
	.err()
	.unwrap_or_else(|| panic!("expected disabled pull request error"));
	assert!(
		open_request_error
			.to_string()
			.contains("OpenReleaseRequest")
	);
	assert!(open_request_error.to_string().contains("enabled` is false"));

	let comment_provider_error = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&sample_source_configuration(SourceProvider::GitLab)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported provider error"));
	assert!(
		comment_provider_error
			.to_string()
			.contains("does not support released-issue comments")
	);
}

#[test]
fn validate_cli_runtime_requirements_enforce_affected_package_inputs() {
	let verify_disabled = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"affected",
			vec![CliStepDefinition::AffectedPackages {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings {
			affected: monochange_core::ChangesetAffectedSettings {
				enabled: false,
				..Default::default()
			},
			..Default::default()
		},
		Some(&sample_source_configuration(SourceProvider::GitHub)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected verify disabled error"));
	assert!(verify_disabled.to_string().contains("AffectedPackages"));
	assert!(verify_disabled.to_string().contains("enabled` is false"));

	let missing_selector = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"affected",
			vec![CliStepDefinition::AffectedPackages {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&sample_source_configuration(SourceProvider::GitHub)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected selector error"));
	assert!(
		missing_selector
			.to_string()
			.contains("declares neither a `changed_paths` nor a `from` input")
	);

	let mut command = cli_command(
		"affected",
		vec![CliStepDefinition::AffectedPackages {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::from([
				("changed_paths".to_string(), CliStepInputValue::Inherited),
				("label".to_string(), CliStepInputValue::Inherited),
			]),
		}],
	);
	command.inputs = vec![
		cli_input("changed_paths", CliInputKind::StringList),
		cli_input("label", CliInputKind::String),
	];
	let invalid_label = crate::validate_cli_runtime_requirements(
		&[command],
		&monochange_core::ChangesetSettings::default(),
		Some(&sample_source_configuration(SourceProvider::GitHub)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid label kind error"));
	assert!(
		invalid_label
			.to_string()
			.contains("inherits input `label` with incompatible type")
	);
}

#[test]
fn validate_package_and_source_settings_cover_duplicate_and_pattern_errors() {
	let root = fixture_path("config/validation-helper-branches");

	let duplicate_path_error = validate_package_and_group_definitions_for_test(
		&root,
		"[package.core]\npath = 'crates/core'\n\n[package.util]\npath = 'crates/core'\n",
		&[
			package_definition("core", "crates/core"),
			package_definition("util", "crates/core"),
		],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate path error"));
	assert!(
		duplicate_path_error
			.to_string()
			.contains("package path `crates/core` is already used by `core`")
	);

	let mut primary_core = package_definition("core", "crates/core");
	primary_core.version_format = VersionFormat::Primary;
	let mut primary_util = package_definition("util", "crates/util");
	primary_util.version_format = VersionFormat::Primary;
	let duplicate_primary_error = validate_package_and_group_definitions_for_test(
		&root,
		"[package.core]\nversion_format = 'primary'\n\n[package.util]\nversion_format = 'primary'\n",
		&[primary_core, primary_util],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate primary error"));
	assert!(
		duplicate_primary_error
			.to_string()
			.contains("`version_format = \"primary\"` is already used by `core`")
	);

	let source_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings {
			affected: monochange_core::ChangesetAffectedSettings {
				changed_paths: vec!["[".to_string()],
				..Default::default()
			},
			..Default::default()
		},
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid source glob error"));
	assert!(
		source_error
			.to_string()
			.contains("[changesets.affected].changed_paths contains invalid glob pattern")
	);

	let package_pattern_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings::default(),
		&[monochange_core::PackageDefinition {
			ignored_paths: vec![String::new()],
			values: BTreeMap::new(),
			display_version: None,
			..package_definition("core", "crates/core")
		}],
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid package path pattern error"));
	assert!(
		package_pattern_error
			.to_string()
			.contains("[package.core].ignored_paths must not include empty values")
	);

	let duplicate_id_error = validate_package_and_group_definitions_for_test(
		&root,
		"[package.core]\npath = 'crates/core'\n",
		&[
			package_definition("core", "crates/core"),
			package_definition("core", "crates/util"),
		],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate id error"));
	assert!(
		duplicate_id_error
			.to_string()
			.contains("duplicate package id `core`")
	);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir core dir: {error}"));
	let missing_manifest_error = validate_package_and_group_definitions_for_test(
		tempdir.path(),
		"[package.core]\npath = 'crates/core'\ntype = 'cargo'\n",
		&[package_definition("core", "crates/core")],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing manifest error"));
	assert!(
		missing_manifest_error
			.to_string()
			.contains("missing expected cargo manifest")
	);
}

#[test]
fn parse_markdown_change_target_and_validation_helpers_cover_remaining_error_paths() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	// The workspace configures `security` and `test`; `docs` is neither declared
	// there nor unknown to monochange, because the built-in vocabulary is
	// inherited. Use a key that exists nowhere.
	let invalid_scalar = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("nope")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let scalar_error = crate::parse_markdown_change_target(
		&invalid_scalar,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid scalar error"));
	assert!(
		scalar_error
			.to_string()
			.contains("invalid scalar change type `nope`")
	);
	assert!(scalar_error.to_string().contains("valid types"));

	let unknown_keys = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("extra: true")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let unknown_keys_error = crate::parse_markdown_change_target(
		&unknown_keys,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported field error"));
	assert!(
		unknown_keys_error
			.to_string()
			.contains("uses unsupported field(s): extra")
	);

	let invalid_bump = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("bump: nope")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let invalid_bump_error = crate::parse_markdown_change_target(
		&invalid_bump,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid bump error"));
	assert!(
		invalid_bump_error
			.to_string()
			.contains("has invalid bump `nope`")
	);

	let invalid_version = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("version: nope")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let invalid_version_error = crate::parse_markdown_change_target(
		&invalid_version,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(
		invalid_version_error
			.to_string()
			.contains("has invalid version `nope`")
	);

	let none_only = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("bump: none")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let none_only_error = crate::parse_markdown_change_target(
		&none_only,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected none-only error"));
	assert!(
		none_only_error
			.to_string()
			.contains("must not use `bump = \"none\"`")
	);

	let none_with_caused_by =
		serde_yaml_ng::from_str::<serde_yaml_ng::Value>("bump: none\ncaused_by: [\"sdk\"]")
			.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let parsed_none_with_caused_by = crate::parse_markdown_change_target(
		&none_with_caused_by,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.unwrap_or_else(|error| panic!("parse none caused_by: {error}"));
	assert_eq!(
		parsed_none_with_caused_by,
		(
			Some(BumpSeverity::None),
			None,
			None,
			vec!["sdk".to_string()]
		)
	);

	let unknown_caused_by =
		serde_yaml_ng::from_str::<serde_yaml_ng::Value>("bump: patch\ncaused_by: [\"missing\"]")
			.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let unknown_caused_by_error = crate::parse_markdown_change_target(
		&unknown_caused_by,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unknown caused_by error"));
	insta::assert_snapshot!(
		"parse_markdown_change_target_unknown_caused_by_error",
		unknown_caused_by_error.to_string()
	);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir core: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.core]\npath = \"crates/core\"\ntype = \"cargo\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
	fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));
	let no_types_configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
	let no_types_error = crate::validate_configured_change_type(
		&no_types_configuration,
		Path::new("change.md"),
		"core",
		"custom_unknown_type",
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid type error"));
	insta::assert_snapshot!(
		"parse_markdown_change_target_no_configured_types_error",
		no_types_error.to_string()
	);
}

#[test]
fn parse_markdown_change_target_covers_caused_by_scalar_and_error_paths() {
	let _guard = snapshot_settings().bind_to_scope();
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let scalar_type = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("security")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let parsed_scalar_type = crate::parse_markdown_change_target(
		&scalar_type,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.unwrap_or_else(|error| panic!("parse scalar type: {error}"));
	assert_eq!(
		parsed_scalar_type,
		(
			Some(BumpSeverity::Patch),
			None,
			Some("security".to_string()),
			Vec::new(),
		)
	);

	let invalid_compound = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("- security")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let invalid_compound_error = crate::parse_markdown_change_target(
		&invalid_compound,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid compound error"));
	insta::assert_snapshot!(
		"parse_markdown_change_target_invalid_compound_error",
		invalid_compound_error.to_string()
	);

	for (label, snapshot_name, yaml) in [
		(
			"empty string",
			"parse_markdown_change_target_empty_caused_by_string_error",
			"bump: patch\ncaused_by: \"   \"",
		),
		(
			"empty sequence",
			"parse_markdown_change_target_empty_caused_by_sequence_error",
			"bump: patch\ncaused_by: []",
		),
		(
			"non-string entry",
			"parse_markdown_change_target_non_string_caused_by_entry_error",
			"bump: patch\ncaused_by: [123]",
		),
		(
			"non-sequence type",
			"parse_markdown_change_target_invalid_caused_by_type_error",
			"bump: patch\ncaused_by: 123",
		),
	] {
		let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(yaml)
			.unwrap_or_else(|error| panic!("yaml parse for {label}: {error}"));
		let error = crate::parse_markdown_change_target(
			&value,
			Path::new("change.md"),
			"core",
			&configuration,
		)
		.err()
		.unwrap_or_else(|| panic!("expected caused_by error for {label}"));
		insta::assert_snapshot!(snapshot_name, error.to_string());
	}
}

#[test]
fn load_change_signals_covers_configured_type_and_caused_by_context_paths() {
	let _guard = snapshot_settings().bind_to_scope();
	let tempdir = setup_fixture("changeset-target-metadata/render-workspace");
	fs::write(
		tempdir.path().join("change.md"),
		"---\ncore: security\napp:\n  bump: none\n  caused_by: sdk\n---\n\n# follow-up\n",
	)
	.unwrap_or_else(|error| panic!("write change.md: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			tempdir.path().join("crates/core/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"app",
			tempdir.path().join("crates/app/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];

	for (package, id) in packages.iter_mut().zip(["core", "app"]) {
		package.id = id.to_string();
	}

	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&tempdir.path().join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	assert_eq!(signals.len(), 2);
	let core_signal = signals
		.iter()
		.find(|signal| signal.package_id == "core")
		.unwrap_or_else(|| panic!("expected core signal"));
	assert_eq!(core_signal.requested_bump, Some(BumpSeverity::Patch));
	assert_eq!(core_signal.change_type.as_deref(), Some("security"));
	let app_signal = signals
		.iter()
		.find(|signal| signal.package_id == "app")
		.unwrap_or_else(|| panic!("expected app signal"));
	assert_eq!(app_signal.requested_bump, Some(BumpSeverity::None));
	assert_eq!(app_signal.caused_by, vec!["sdk".to_string()]);

	fs::write(
		tempdir.path().join("invalid-change.md"),
		"---\ncore:\n  - invalid\n---\n\n# invalid\n",
	)
	.unwrap_or_else(|error| panic!("write invalid-change.md: {error}"));
	let error = load_change_signals(
		&tempdir.path().join("invalid-change.md"),
		&configuration,
		&packages,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid compound target error"));
	insta::assert_snapshot!(
		"load_change_signals_invalid_compound_target_error",
		error.to_string()
	);

	fs::write(
		tempdir.path().join("unknown-target.md"),
		"---\nunknown: note\n---\n\n# unknown\n",
	)
	.unwrap_or_else(|error| panic!("write unknown-target.md: {error}"));
	let unknown_target_error = load_change_signals(
		&tempdir.path().join("unknown-target.md"),
		&configuration,
		&packages,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unknown target error"));
	insta::assert_snapshot!(
		"load_change_signals_unknown_target_error",
		unknown_target_error.to_string()
	);
}

#[test]
fn validate_versioned_files_and_release_notes_cover_remaining_validation_paths() {
	let root = fixture_path("config/validation-helper-branches");
	let config_contents = "[package.core]\npath = 'crates/core'\n";
	let declared_packages = std::collections::BTreeSet::from(["core"]);

	let missing_type = validate_versioned_files_for_test(
		&root,
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "README.md".to_string(),
			ecosystem_type: None,
			format: None,
			name: None,
			fields: None,
			prefix: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing type error"));
	assert!(
		missing_type
			.to_string()
			.contains("versioned_files must set `type`")
	);

	let invalid_glob = validate_versioned_files_for_test(
		&root,
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "[".to_string(),
			ecosystem_type: Some(EcosystemType::Cargo),
			format: None,
			name: None,
			fields: None,
			prefix: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid glob error"));
	assert!(
		invalid_glob
			.to_string()
			.contains("invalid glob pattern `[`")
	);

	let duplicate_glob_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(duplicate_glob_dir.path().join("packages/web"))
		.unwrap_or_else(|error| panic!("mkdir duplicate glob fixture: {error}"));
	fs::write(
		duplicate_glob_dir.path().join("packages/web/package.json"),
		"{\"name\":\"web\",\"version\":\"1.0.0\"}\n",
	)
	.unwrap_or_else(|error| panic!("write duplicate glob package: {error}"));
	let duplicate_glob = monochange_core::VersionedFileDefinition {
		path: "packages/**/package.json".to_string(),
		ecosystem_type: Some(EcosystemType::Npm),
		format: None,
		name: None,
		fields: None,
		prefix: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	validate_versioned_files_for_test(
		duplicate_glob_dir.path(),
		config_contents,
		&[duplicate_glob.clone(), duplicate_glob],
		&declared_packages,
		"package",
		"core",
	)
	.unwrap_or_else(|error| panic!("duplicate glob validation should be cached: {error}"));

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("packages/web"))
		.unwrap_or_else(|error| panic!("mkdir web package: {error}"));
	fs::write(
		tempdir.path().join("packages/web/package.json"),
		"{\"name\":\"web\",\"version\":\"1.0.0\"}\n",
	)
	.unwrap_or_else(|error| panic!("write package.json: {error}"));
	let unsupported_match = validate_versioned_files_for_test(
		tempdir.path(),
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "packages/*/package.json".to_string(),
			ecosystem_type: Some(EcosystemType::Cargo),
			format: None,
			name: None,
			fields: None,
			prefix: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported match error"));
	assert!(
		unsupported_match
			.to_string()
			.contains("matched unsupported file")
	);
	assert!(
		unsupported_match
			.to_string()
			.contains("for ecosystem `cargo`")
	);

	assert!(crate::path_is_supported_for_ecosystem(
		Path::new("pnpm-lock.yaml"),
		EcosystemType::Npm
	));
	assert!(crate::path_is_supported_for_ecosystem(
		Path::new("yarn.lock"),
		EcosystemType::Npm
	));
	assert!(crate::path_is_supported_for_ecosystem(
		Path::new("bun.lock"),
		EcosystemType::Npm
	));
	assert!(crate::path_is_supported_for_ecosystem(
		Path::new("pubspec.yaml"),
		EcosystemType::Dart
	));
	let dart_unsupported_match = validate_versioned_files_for_test(
		tempdir.path(),
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "packages/*/package.json".to_string(),
			ecosystem_type: Some(EcosystemType::Dart),
			format: None,
			name: None,
			fields: None,
			prefix: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported dart match error"));
	assert!(
		dart_unsupported_match
			.to_string()
			.contains("for ecosystem `dart`")
	);

	assert!(crate::path_is_supported_for_ecosystem(
		Path::new("go.mod"),
		EcosystemType::Go
	));
	let go_unsupported_match = validate_versioned_files_for_test(
		tempdir.path(),
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "packages/*/package.json".to_string(),
			ecosystem_type: Some(EcosystemType::Go),
			format: None,
			name: None,
			fields: None,
			prefix: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported go match error"));
	assert!(
		go_unsupported_match
			.to_string()
			.contains("for ecosystem `go`")
	);

	let empty_template = crate::validate_changelog_configuration(
		"",
		&crate::RawChangelogSettings {
			templates: vec![" ".to_string()],
			sections: BTreeMap::new(),
			section_thresholds: monochange_core::ChangelogSectionThresholds::default(),
			types: BTreeMap::new(),
			streams: BTreeMap::new(),
			outputs: BTreeMap::new(),
			style: monochange_core::ChangelogStyle::default(),
			release_notes: monochange_core::ReleaseNotesStyleOverrides::default(),
		},
		&[],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected empty template error"));
	assert!(
		empty_template
			.to_string()
			.contains("must not include empty templates")
	);

	assert_eq!(
		crate::change_template_variables("{{ summary }} {{ details | default('') }} {{"),
		vec!["details".to_string(), "summary".to_string()]
	);

	let invalid_thresholds = crate::validate_changelog_configuration(
		"",
		&crate::RawChangelogSettings {
			templates: Vec::new(),
			sections: BTreeMap::new(),
			section_thresholds: monochange_core::ChangelogSectionThresholds {
				collapse: 80,
				ignored: 50,
			},
			types: BTreeMap::new(),
			streams: BTreeMap::new(),
			outputs: BTreeMap::new(),
			style: monochange_core::ChangelogStyle::default(),
			release_notes: monochange_core::ReleaseNotesStyleOverrides::default(),
		},
		&[],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid threshold error"));
	assert!(
		invalid_thresholds
			.to_string()
			.contains("section_thresholds.ignored")
	);
}

#[test]
fn validate_github_source_and_api_configuration_cover_remaining_paths() {
	let github_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: ProviderMergeRequestSettings {
				labels: vec![" ".to_string()],
				..Default::default()
			},
		}))
		.err()
		.unwrap_or_else(|| panic!("expected invalid github labels error"));
	assert!(
		github_error
			.to_string()
			.contains("[source.pull_requests].labels must not include empty values")
	);

	let source_api_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: Some("https://example.invalid".to_string()),
			api_url: Some("http://api.example.invalid".to_string()),
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: ProviderMergeRequestSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected insecure api_url error"));
	assert!(source_api_error.to_string().contains("insecure scheme"));

	crate::validate_api_url_host("https://github.example.com/api/v3", SourceProvider::GitHub)
		.unwrap_or_else(|error| panic!("custom GitHub host should warn but succeed: {error}"));
}

#[test]
fn matching_package_helpers_cover_references_and_definitions() {
	let root = PathBuf::from("/workspace");
	let core = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let web = PackageRecord::new(
		Ecosystem::Npm,
		"web",
		root.join("packages/web/package.json"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let packages = vec![core.clone(), web.clone()];
	assert_eq!(
		crate::find_matching_package_indices(&packages, &root, "core"),
		vec![0]
	);
	assert_eq!(
		crate::find_matching_package_indices(&packages, &root, "packages/web"),
		vec![1]
	);
	let definition = monochange_core::PackageDefinition {
		id: "web".to_string(),
		path: PathBuf::from("packages/web"),
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
		version_format: VersionFormat::Primary,
		version_source: VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
		publish: monochange_core::PublishSettings::default(),
		cli: None,

		values: BTreeMap::new(),
		display_version: None,
	};
	assert_eq!(
		crate::find_matching_package_indices_for_definition(&packages, &root, &definition),
		vec![1]
	);
	assert!(crate::package_matches_definition(&web, &root, &definition));
	assert!(!crate::package_matches_definition(
		&core,
		&root,
		&definition
	));
	assert!(crate::ecosystem_matches_package_type(
		Ecosystem::Dart,
		PackageType::Dart
	));
}

// -- validate_versioned_files_content tests --
/// A test double for one ecosystem adapter.
///
/// `monochange_config` cannot depend on the ecosystem crates, so these tests
/// pin the contract the registry carries: dispatch by ecosystem, fields passed
/// through untouched, and adapter errors wrapped with owner context. The
/// per-format parsing behaviour lives in the ecosystem crates.
struct RecordingAdapter {
	ecosystem: Ecosystem,
	result: Result<(), String>,
}

impl EcosystemAdapter for RecordingAdapter {
	fn ecosystem(&self) -> Ecosystem {
		self.ecosystem
	}

	fn discover(&self, _root: &Path) -> MonochangeResult<monochange_core::AdapterDiscovery> {
		Ok(monochange_core::AdapterDiscovery {
			packages: Vec::new(),
			warnings: Vec::new(),
		})
	}

	fn load_configured(
		&self,
		_root: &Path,
		_package_path: &Path,
	) -> MonochangeResult<Option<PackageRecord>> {
		Ok(None)
	}

	fn supported_versioned_file_kind(&self, _path: &Path) -> bool {
		true
	}

	fn validate_versioned_file(
		&self,
		_full_path: &Path,
		_display_path: &str,
		_custom_fields: Option<&[String]>,
	) -> MonochangeResult<()> {
		self.result.clone().map_err(MonochangeError::Config)
	}
}

fn registry_with(ecosystem: Ecosystem, result: Result<(), String>) -> EcosystemRegistry {
	let mut registry = EcosystemRegistry::new();
	registry.push_adapter(Box::new(RecordingAdapter { ecosystem, result }));
	registry
}

#[test]
fn validate_versioned_files_content_wraps_adapter_errors_with_owner_context() {
	let root = fixture_path("config/versioned-file-unparseable-version");
	let ecosystems = registry_with(
		Ecosystem::Cargo,
		Err("versioned file `crates/core/Cargo.toml` does not contain a readable version field (checked: package.version, workspace.package.version, version)".to_string()),
	);
	let error = crate::validate_versioned_files_content(&root, &ecosystems)
		.err()
		.unwrap_or_else(|| panic!("expected adapter failure to surface"));
	let rendered = error.to_string();
	assert!(
		rendered.contains("package `core` versioned file `crates/core/Cargo.toml`"),
		"expected owner context in `{rendered}`"
	);
	assert!(
		rendered.contains("does not contain a readable version field"),
		"expected adapter detail in `{rendered}`"
	);
}

#[test]
fn validate_versioned_files_content_warns_once_per_owner_on_duplicate_paths() {
	let root = fixture_path("config/versioned-file-duplicate-path");
	let warnings = crate::validate_versioned_files_content(&root, &EcosystemRegistry::new())
		.unwrap_or_else(|error| panic!("expected Ok with warnings, got error: {error}"));
	assert_eq!(
		warnings.len(),
		1,
		"exact duplicates and cross-owner sharing must stay silent, got: {warnings:?}"
	);
	let warning = warnings.first().unwrap();
	assert!(
		warning.contains(
			"package `core` declares multiple versioned file entries for `deploy/artifact.json`"
		),
		"got: {warning}"
	);
	assert!(warning.contains("declaration order"), "got: {warning}");
}

// -- proptest_config module (merged from proptest_config.rs) --
use proptest::prelude::*;

use crate::render_changelog_path_template;

proptest! {
	#[test]
	fn package_dir_always_replaced_with_directory_name(
		prefix in any::<String>(),
		suffix in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!prefix.contains("{{") && !prefix.contains("}}"));
		prop_assume!(!suffix.contains("{{") && !suffix.contains("}}"));
		prop_assume!(!path_str.is_empty());

		let template = format!("{prefix}{{{{package_dir}}}}{suffix}");
		let path = Path::new(&path_str);
		let result = render_changelog_path_template(&template, path);

		let expected_dir = path
			.file_name()
			.map(|s| s.to_string_lossy().into_owned())
			.unwrap_or_default();

		prop_assert!(!result.contains("{{package_dir}}"));
		prop_assert_eq!(result, format!("{prefix}{expected_dir}{suffix}"));
	}

	#[test]
	fn package_name_always_replaced_with_file_stem(
		prefix in any::<String>(),
		suffix in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!prefix.contains("{{") && !prefix.contains("}}"));
		prop_assume!(!suffix.contains("{{") && !suffix.contains("}}"));
		prop_assume!(!path_str.is_empty());

		let template = format!("{prefix}{{{{package_name}}}}{suffix}");
		let path = Path::new(&path_str);
		let result = render_changelog_path_template(&template, path);

		let expected_stem = path
			.file_stem()
			.map(|s| s.to_string_lossy().into_owned())
			.unwrap_or_default();

		prop_assert!(!result.contains("{{package_name}}"));
		prop_assert_eq!(result, format!("{prefix}{expected_stem}{suffix}"));
	}

	#[test]
	fn unknown_placeholders_are_left_unchanged(
		prefix in any::<String>(),
		suffix in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!prefix.contains("{{") && !prefix.contains("}}"));
		prop_assume!(!suffix.contains("{{") && !suffix.contains("}}"));

		let template = format!("{prefix}{{{{unknown}}}}{suffix}");
		let path = Path::new(&path_str);
		let result = render_changelog_path_template(&template, path);

		prop_assert_eq!(result, template);
	}

	#[test]
	fn idempotent_when_no_placeholders_exist(
		template in any::<String>(),
		path_str in any::<String>()
	) {
		prop_assume!(!template.contains("{{"));

		let path = Path::new(&path_str);
		let once = render_changelog_path_template(&template, path);
		let twice = render_changelog_path_template(&once, path);

		prop_assert_eq!(once, twice);
	}

	#[test]
	fn infer_bump_from_versions_returns_major_when_major_increases(
		current_major in 0u64..20,
		current_minor in 0u64..20,
		current_patch in 0u64..20,
		next_major_delta in 1u64..5,
	) {
		let current = Version::new(current_major, current_minor, current_patch);
		let explicit = Version::new(
			current_major + next_major_delta,
			current_minor,
			current_patch,
		);

		prop_assert_eq!(crate::infer_bump_from_versions(&current, &explicit), BumpSeverity::Major);
	}

	#[test]
	fn infer_bump_from_versions_returns_minor_when_minor_increases(
		current_major in 0u64..20,
		current_minor in 0u64..20,
		current_patch in 0u64..20,
		next_minor_delta in 1u64..5,
	) {
		let current = Version::new(current_major, current_minor, current_patch);
		let explicit = Version::new(
			current_major,
			current_minor + next_minor_delta,
			current_patch,
		);

		prop_assert_eq!(crate::infer_bump_from_versions(&current, &explicit), BumpSeverity::Minor);
	}

	#[test]
	fn infer_bump_from_versions_returns_patch_when_only_patch_increases(
		current_major in 0u64..20,
		current_minor in 0u64..20,
		current_patch in 0u64..20,
		next_patch_delta in 1u64..5,
	) {
		let current = Version::new(current_major, current_minor, current_patch);
		let explicit = Version::new(
			current_major,
			current_minor,
			current_patch + next_patch_delta,
		);

		prop_assert_eq!(crate::infer_bump_from_versions(&current, &explicit), BumpSeverity::Patch);
	}
}

fn source_config_for_provider(provider: SourceProvider) -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider,
		owner: "owner".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

#[test]
fn source_provider_capabilities_reject_unsupported_settings() {
	let gitea_without_host = source_config_for_provider(SourceProvider::Gitea);
	let error = crate::validate_source_provider_capabilities(&gitea_without_host).unwrap_err();
	assert!(error.to_string().contains("[source].host must be set"));

	let mut gitlab_with_draft = source_config_for_provider(SourceProvider::GitLab);
	gitlab_with_draft.releases.draft = true;
	let error = crate::validate_source_provider_capabilities(&gitlab_with_draft).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("[source.releases].draft is not supported")
	);

	let mut gitlab_with_prerelease = source_config_for_provider(SourceProvider::GitLab);
	gitlab_with_prerelease.releases.prerelease = true;
	let error = crate::validate_source_provider_capabilities(&gitlab_with_prerelease).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("[source.releases].prerelease is not supported")
	);

	let mut gitea_with_notes = source_config_for_provider(SourceProvider::Gitea);
	gitea_with_notes.host = Some("https://git.example.com".to_string());
	gitea_with_notes.releases.generate_notes = true;
	let error = crate::validate_source_provider_capabilities(&gitea_with_notes).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("provider-generated release notes are not supported")
	);

	let mut gitea_with_auto_merge = source_config_for_provider(SourceProvider::Gitea);
	gitea_with_auto_merge.host = Some("https://git.example.com".to_string());
	gitea_with_auto_merge.pull_requests.auto_merge = true;
	let error = crate::validate_source_provider_capabilities(&gitea_with_auto_merge).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("[source.pull_requests].auto_merge is not supported")
	);
}

#[test]
fn load_workspace_configuration_rejects_invalid_prerelease_settings() {
	let cases = [
		(
			"[prerelease]\nchannel = \" \"\n",
			"[prerelease].channel must not be empty",
		),
		(
			"[prerelease]\nchannel = \"bad channel\"\n",
			"[prerelease].channel must be a valid SemVer prerelease identifier",
		),
		(
			"[prerelease]\nbase = \"fixed\"\n",
			"[prerelease].base_version is required when base is `fixed`",
		),
		(
			"[prerelease]\nbase = \"fixed\"\nbase_version = \"1.0.0-alpha.1\"\n",
			"[prerelease].base_version must be a stable SemVer version",
		),
		(
			"[prerelease]\nbase = \"planned\"\nbase_version = \"1.0.0\"\n",
			"[prerelease].base_version is only valid when base is `fixed`",
		),
		(
			"[prerelease]\nbranches = [\"next\", \" \"]\n",
			"[prerelease].branches must not contain empty branch patterns",
		),
	];

	for (config, expected) in cases {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::write(tempdir.path().join("monochange.toml"), config)
			.unwrap_or_else(|error| panic!("write config: {error}"));

		let error = load_workspace_configuration(tempdir.path())
			.expect_err("invalid prerelease configuration should fail");
		assert!(
			error.to_string().contains(expected),
			"expected `{expected}` in `{error}`"
		);
	}
}

#[test]
fn validate_versioned_files_accepts_format_mode_and_rejects_invalid_combinations() {
	let root = fixture_path("config/validation-helper-branches");
	let config_contents = "[package.core]\npath = 'crates/core'\n";
	let declared_packages = std::collections::BTreeSet::from(["core"]);
	let valid = monochange_core::VersionedFileDefinition {
		path: "metadata.json".to_string(),
		ecosystem_type: None,
		format: Some(monochange_core::VersionedFileFormat::Json),
		name: None,
		fields: Some(vec!["release.version".to_string()]),
		prefix: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	validate_versioned_files_for_test(
		&root,
		config_contents,
		&[valid],
		&declared_packages,
		"package",
		"core",
	)
	.unwrap_or_else(|error| panic!("format versioned file should validate: {error}"));

	let mixed_type = monochange_core::VersionedFileDefinition {
		path: "metadata.json".to_string(),
		ecosystem_type: Some(EcosystemType::Npm),
		format: Some(monochange_core::VersionedFileFormat::Json),
		name: None,
		fields: Some(vec!["release.version".to_string()]),
		prefix: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = validate_versioned_files_for_test(
		&root,
		config_contents,
		&[mixed_type],
		&declared_packages,
		"package",
		"core",
	)
	.expect_err("format and type should be mutually exclusive");
	assert!(error.to_string().contains("cannot also set `type`"));

	let mixed_regex = monochange_core::VersionedFileDefinition {
		path: "metadata.json".to_string(),
		ecosystem_type: None,
		format: Some(monochange_core::VersionedFileFormat::Json),
		name: None,
		fields: Some(vec!["release.version".to_string()]),
		prefix: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: Some("version = (?<version>.*)".to_string()),
		value_template: None,
	};
	let error = validate_versioned_files_for_test(
		&root,
		config_contents,
		&[mixed_regex],
		&declared_packages,
		"package",
		"core",
	)
	.expect_err("format and regex should be mutually exclusive");
	assert!(error.to_string().contains("cannot also set `regex`"));

	let missing_fields = monochange_core::VersionedFileDefinition {
		path: ".env".to_string(),
		ecosystem_type: None,
		format: Some(monochange_core::VersionedFileFormat::Env),
		name: None,
		fields: None,
		prefix: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = validate_versioned_files_for_test(
		&root,
		config_contents,
		&[missing_fields],
		&declared_packages,
		"package",
		"core",
	)
	.expect_err("format mode should require fields");
	assert!(error.to_string().contains("must set `fields`"));

	let empty_fields = monochange_core::VersionedFileDefinition {
		path: ".env".to_string(),
		ecosystem_type: None,
		format: Some(monochange_core::VersionedFileFormat::Env),
		name: None,
		fields: Some(Vec::new()),
		prefix: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = validate_versioned_files_for_test(
		&root,
		config_contents,
		&[empty_fields],
		&declared_packages,
		"package",
		"core",
	)
	.expect_err("format mode should reject empty fields");
	assert!(error.to_string().contains("fields must be non-empty"));
}

#[test]
fn auto_discover_rejects_invalid_patterns_in_every_ecosystem() {
	let root = fixture_path("config/auto-discover-invalid-include");
	let mut errors = Vec::new();

	for ecosystem in [
		EcosystemType::Cargo,
		EcosystemType::Npm,
		EcosystemType::Deno,
		EcosystemType::Dart,
		EcosystemType::Python,
		EcosystemType::Go,
	] {
		for (include, exclude) in [
			(vec!["crates/[", "crates/*"], Vec::new()),
			(vec!["crates/*"], vec!["crates/["]),
			(Vec::new(), vec!["crates/["]),
		] {
			let settings = AutoDiscoverSettings {
				include: include.into_iter().map(String::from).collect(),
				exclude: exclude.into_iter().map(String::from).collect(),
				id: monochange_core::default_auto_discover_id(),
				defaults: AutoDiscoverPackageDefaults::default(),
			};
			let error = crate::discover_packages_from_ecosystem(&root, ecosystem, &settings)
				.expect_err("invalid glob must fail rather than silently change ownership");
			errors.push(error.to_string());
		}
	}

	insta::assert_json_snapshot!(errors);
}

#[test]
fn load_workspace_configuration_auto_discovers_cargo_packages_and_explicit_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	for (path, name, version) in [
		("crates/core", "core-lib", "1.0.0"),
		("crates/util", "util-lib", "2.0.0"),
		("crates/ignored", "ignored-lib", "3.0.0"),
	] {
		let package_dir = root.join(path);
		fs::create_dir_all(&package_dir)
			.unwrap_or_else(|error| panic!("create package dir: {error}"));
		fs::write(
			package_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{name}\"\nversion = \"{version}\"\n"),
		)
		.unwrap_or_else(|error| panic!("write manifest: {error}"));
	}

	fs::write(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = "{{ path }}/CHANGELOG.md"

[ecosystems.cargo]
versioned_files = ["Cargo.toml"]

[ecosystems.cargo.auto_discover]
include = ["crates/*"]
exclude = ["crates/ignored"]

[package.core-lib]
path = "crates/core"
type = "cargo"
tag = false
release = false
version_format = "primary"
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));

	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));
	let core_package = configuration
		.packages
		.iter()
		.find(|package| package.id == "core-lib")
		.unwrap_or_else(|| panic!("missing explicit package"));
	let util_package = configuration
		.packages
		.iter()
		.find(|package| package.id == "util-lib")
		.unwrap_or_else(|| panic!("missing auto-discovered package"));

	assert_eq!(configuration.packages.len(), 2);
	assert_eq!(core_package.path, PathBuf::from("crates/core"));
	assert!(!core_package.tag);
	assert!(!core_package.release);
	assert_eq!(util_package.path, PathBuf::from("crates/util"));
	assert!(util_package.tag);
	assert!(util_package.release);
	assert_eq!(
		util_package.ignored_paths,
		vec![PathBuf::from("crates/ignored")]
	);

	assert_eq!(util_package.versioned_files.len(), 1);
	assert_eq!(util_package.version_format, VersionFormat::Namespaced);
	assert_eq!(
		util_package
			.changelog
			.as_ref()
			.unwrap_or_else(|| panic!("missing changelog"))
			.path,
		PathBuf::from("crates/util/CHANGELOG.md")
	);
	assert!(
		configuration
			.packages
			.iter()
			.all(|package| package.id != "ignored-lib")
	);
}

#[test]
fn load_workspace_configuration_rejects_semver_checks_for_non_cargo_ecosystems() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r"
[ecosystems.npm.semver_checks]
enabled = true
",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));

	let error = load_workspace_configuration(tempdir.path())
		.expect_err("non-Cargo semver checks should fail");

	assert!(
		error
			.to_string()
			.contains("`ecosystems.npm.semver_checks` is only supported for Cargo")
	);
}

#[test]
fn load_workspace_configuration_validates_semver_matrix_bounds_and_values() {
	let too_many_cells = (0..17).fold(String::new(), |mut contents, index| {
		writeln!(
			contents,
			"[[ecosystems.cargo.semver_checks.matrix]]\nname = \"cell-{index}\""
		)
		.unwrap_or_else(|error| panic!("write matrix case: {error}"));
		contents
	});
	let cases = [
		(
			"[ecosystems.cargo.semver_checks]\nenabled = true\ntimeout_seconds = 0\n",
			"`timeout_seconds` must be between 1 and 1800",
		),
		(
			"[ecosystems.cargo.semver_checks]\nenabled = true\nmatrix = []\n",
			"`matrix` must contain at least one feature and target combination",
		),
		(
			too_many_cells.as_str(),
			"`matrix` cannot contain more than 16 combinations",
		),
		(
			"[[ecosystems.cargo.semver_checks.matrix]]\nname = \"bad-target\"\ntarget = \"  \"\n",
			"matrix cell `bad-target` has an empty `target`",
		),
		(
			"[[ecosystems.cargo.semver_checks.matrix]]\nname = \"  \"\n",
			"every matrix cell must have a non-empty `name`",
		),
		(
			"[[ecosystems.cargo.semver_checks.matrix]]\nname = \"bad-feature\"\nfeatures = [\"serde,derive\"]\n",
			"matrix cell `bad-feature` contains an empty feature or a feature with a comma",
		),
	];

	for (contents, expected) in cases {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::write(tempdir.path().join("monochange.toml"), contents)
			.unwrap_or_else(|error| panic!("write config: {error}"));
		let error = load_workspace_configuration(tempdir.path())
			.expect_err("invalid semver matrix should fail");
		assert!(
			error.to_string().contains(expected),
			"expected `{expected}` in `{error}`"
		);
	}
}

#[test]
fn auto_discover_manifest_name_helpers_cover_comments_and_missing_values() {
	assert_eq!(
		crate::extract_manifest_name(
			"[workspace]\nmembers = []\n\n[package]\n# comment\nname = \"core-lib\"\n",
			EcosystemType::Cargo,
		),
		Some("core-lib".to_string())
	);
	assert_eq!(
		crate::extract_manifest_name("[package]\nname = \"unterminated\n", EcosystemType::Cargo),
		None
	);
	assert_eq!(
		crate::extract_manifest_name("{\n  \"version\": \"1.0.0\"\n}", EcosystemType::Npm),
		None
	);
	assert_eq!(
		crate::extract_manifest_name("{\n  \"name\": 123\n}", EcosystemType::Npm),
		None
	);
	assert_eq!(
		crate::extract_manifest_name("\n# package name\nname: 'dart_app'\n", EcosystemType::Dart),
		Some("dart_app".to_string())
	);
	assert_eq!(
		crate::extract_manifest_name("version: 1.0.0\nname: too-late\n", EcosystemType::Dart),
		None
	);
	assert_eq!(
		crate::render_auto_discover_id(
			"{{ ecosystem }}:{{ name }}:{{ path }}:{{ sanitized_path }}:{{ manifest }}",
			"core-lib",
			Path::new("crates/core lib"),
			Path::new("crates/core lib/Cargo.toml"),
			EcosystemType::Cargo,
		),
		"cargo:core-lib:crates/core lib:crates__core_lib:crates/core lib/Cargo.toml"
	);
	assert_eq!(crate::ecosystem_name(EcosystemType::Dart), "dart");

	assert_eq!(crate::ecosystem_name(EcosystemType::Python), "python");
}

#[test]
fn load_workspace_configuration_rejects_primary_custom_version_format_collision() {
	let root = fixture_path("config/rejects-primary-custom-version-format-collision");
	let error = load_workspace_configuration(&root)
		.expect_err("primary and equivalent custom version formats should be rejected")
		.render();
	assert!(error.contains("renders the same sample tag `v0.0.0`"));
	assert!(error.contains("include `{{ name }}`"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_custom_version_format_tags() {
	let root = fixture_path("config/rejects-invalid-custom-version-format");
	let error = load_workspace_configuration(&root)
		.expect_err("invalid custom version format tags should be rejected")
		.render();
	assert!(error.contains("contains whitespace"));
}

#[test]
fn load_workspace_configuration_rejects_defaults_bump_propagation_max_without_inherit() {
	let root = fixture_path("config/bump-propagation-invalid-defaults");
	let error = load_workspace_configuration(&root)
		.expect_err("expected defaults bump_propagation_max validation error");
	assert!(
		error.to_string().contains(
			"defaults `defaults` sets `bump_propagation_max` without `bump_propagation = \"inherit\"`"
		),
		"unexpected error: {error}"
	);
}

#[test]
fn github_actions_package_type_applies_preset_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join("action.yml"), "name: my-action\n")
		.unwrap_or_else(|error| panic!("write action.yml: {error}"));
	fs::write(
		root.join("monochange.toml"),
		"[package.actions]\npath = \".\"\ntype = \"github_actions\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("load config: {error}"));
	let package = &configuration.packages[0];

	assert_eq!(package.package_type, PackageType::GitHubActions);
	assert_eq!(package.version_source, VersionSource::Tag);
	assert!(package.tag);
	assert!(package.release);
	assert!(!package.publish.enabled);
	assert_eq!(package.initial_version, Some(Version::new(0, 1, 0)));
	assert_eq!(
		package.floating_tags,
		vec![
			FloatingTagFormat("v{{ major }}.{{ minor }}".to_string()),
			FloatingTagFormat("v{{ major }}".to_string()),
		]
	);
}

#[test]
fn floating_tags_reject_unsupported_template_variables() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("Cargo.toml"),
		"[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));
	fs::write(
		root.join("monochange.toml"),
		"[package.demo]\npath = \".\"\ntype = \"cargo\"\nfloating_tags = [\"v{{ beta }}\"]\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let error = load_workspace_configuration(root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error.to_string().contains("floating_tags"), "{error}");
}

fn cli_registration_workspace(root: &Path, packages: &[(&str, &str)]) {
	let mut config = String::new();

	for (id, cli_line) in packages {
		let package_dir = root.join("crates").join(id);
		fs::create_dir_all(&package_dir)
			.unwrap_or_else(|error| panic!("create package dir: {error}"));
		fs::write(
			package_dir.join("Cargo.toml"),
			format!("[package]\nname = \"{id}\"\nversion = \"0.1.0\"\n"),
		)
		.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));
		let _ = write!(
			config,
			"[package.{id}]\npath = \"crates/{id}\"\ntype = \"cargo\"\n"
		);

		if !cli_line.is_empty() {
			config.push_str(cli_line);

			config.push('\n');
		}
	}

	fs::write(root.join("monochange.toml"), config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
}

#[test]
fn load_workspace_configuration_rejects_empty_package_cli_name() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(
		tempdir.path(),
		&[(
			"demo",
			"cli = { name = \"\", snapshot = \"demo snapshot\" }",
		)],
	);

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("cli registration must provide a non-empty name"),
		"{error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_package_cli_snapshot_command() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(
		tempdir.path(),
		&[("demo", "cli = { name = \"demo\", snapshot = \"   \" }")],
	);

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("must provide a non-empty snapshot command"),
		"{error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_duplicate_package_cli_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(
		tempdir.path(),
		&[
			(
				"first",
				"cli = { name = \"shared\", snapshot = \"first snapshot\" }",
			),
			(
				"second",
				"cli = { name = \"shared\", snapshot = \"second snapshot\" }",
			),
		],
	);

	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("cli name `shared` is registered by both `first` and `second`"),
		"{error}"
	);
}

#[test]
fn native_auto_discovery_maps_types_and_inherits_versioned_files() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/native-audit");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("load native auto-discovery: {error}"));
	assert_eq!(configuration.packages.len(), 3);

	for package in &configuration.packages {
		assert!(matches!(
			package.package_type,
			PackageType::Swift | PackageType::CocoaPods | PackageType::Gradle
		));
		assert_eq!(package.versioned_files.len(), 1);
	}
}

fn shared_ecosystem_packages(root: &Path) -> Vec<PackageRecord> {
	[
		(Ecosystem::Cargo, "crates/core/Cargo.toml", "shared"),
		(
			Ecosystem::Dart,
			"packages/shared/pubspec.yaml",
			"dart:shared",
		),
	]
	.into_iter()
	.map(|(ecosystem, manifest, config_id)| {
		let mut package = PackageRecord::new(
			ecosystem,
			"shared",
			root.join(manifest),
			root.to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		);
		package
			.metadata
			.insert("config_id".into(), config_id.into());
		package
	})
	.collect()
}

#[test]
fn configured_id_precedence_keeps_name_fallbacks_and_ambiguity_checks() {
	let mut configuration =
		load_workspace_configuration(&fixture_path("config/accepts-mixed-changeset"))
			.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.root_path = configuration
		.root_path
		.canonicalize()
		.unwrap_or_else(|error| panic!("fixture root: {error}"));
	let root = &configuration.root_path;
	let mut packages = shared_ecosystem_packages(root);
	packages[0].metadata.clear();
	let context = crate::build_changeset_load_context(&configuration, &packages);
	assert!(resolve_package_reference("shared", root, &packages).is_err());

	assert!(crate::resolve_package_reference_with_context("shared", &context).is_err());
	packages.truncate(1);
	let context = crate::build_changeset_load_context(&configuration, &packages);
	assert_eq!(
		resolve_package_reference("shared", root, &packages)
			.unwrap_or_else(|error| panic!("unique name: {error}")),
		packages[0].id
	);
	assert_eq!(
		crate::resolve_package_reference_with_context("shared", &context)
			.unwrap_or_else(|error| panic!("unique name: {error}")),
		packages[0].id
	);

	let mut packages = shared_ecosystem_packages(root);
	packages[1]
		.metadata
		.insert("config_id".into(), "shared".into());
	let context = crate::build_changeset_load_context(&configuration, &packages);
	assert!(resolve_package_reference("shared", root, &packages).is_err());
	assert!(crate::resolve_package_reference_with_context("shared", &context).is_err());
}
