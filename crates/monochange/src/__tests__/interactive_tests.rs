#![allow(clippy::disallowed_methods)]
use std::collections::BTreeMap;

use inquire::validator::Validation;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangelogSettings;
use monochange_core::ChangesetSettings;
use monochange_core::EcosystemSettings;
use monochange_core::GroupChangelogInclude;
use monochange_core::GroupDefinition;
use monochange_core::MonochangeError;
use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::PublishSettings;
use monochange_core::VersionFormat;
use monochange_core::VersionSource;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use monochange_test_helpers::current_test_name;

use super::InteractiveOptions;
use super::SelectableTarget;
use super::TargetKind;
use super::build_selectable_targets;
use super::bump_options;
use super::bump_prompt_label;
use super::change_type_options;
use super::change_type_prompt_label;
use super::map_inquire_error;
use super::normalize_optional_text;
use super::parse_change_type_selection;
use super::parse_selected_bump;
use super::prompt_change_type_for_target;
use super::render_group_target_display;
use super::run_interactive_change;
use super::validate_reason_input;
use super::validate_semver_input;
use super::version_prompt_label;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn package_target(configured_types: Vec<String>) -> SelectableTarget {
	SelectableTarget {
		id: "core".to_string(),
		kind: TargetKind::Package,
		display: "core".to_string(),
		configured_types,
	}
}

fn empty_configuration() -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		prerelease: monochange_core::PrereleaseConfiguration::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
		python: EcosystemSettings::default(),
		go: EcosystemSettings::default(),
		swift: EcosystemSettings::default(),
		cocoapods: EcosystemSettings::default(),
		gradle: EcosystemSettings::default(),
		version_schemes: BTreeMap::new(),
	}
}

#[test]
fn prompt_label_helpers_cover_package_and_group_variants() {
	let package = package_target(Vec::new());
	let group = SelectableTarget {
		id: "sdk".to_string(),
		kind: TargetKind::Group,
		display: "sdk".to_string(),
		configured_types: vec!["docs".to_string()],
	};
	assert_eq!(bump_prompt_label(&package), "Bump for package `core`:");
	assert_eq!(bump_prompt_label(&group), "Bump for group `sdk`:");
	assert_eq!(
		version_prompt_label(&package),
		"Pin explicit version for `core`? (leave empty to skip):"
	);
	assert_eq!(
		version_prompt_label(&group),
		"Pin explicit version for group `sdk`? (leave empty to skip):"
	);
	assert_eq!(
		change_type_prompt_label(&package),
		"Change type for `core`:"
	);
	assert_eq!(
		change_type_prompt_label(&group),
		"Change type for group `sdk`:"
	);
}

#[test]
fn validation_helpers_cover_semver_reason_and_optional_normalization() {
	assert!(matches!(validate_semver_input(""), Validation::Valid));
	assert!(matches!(validate_semver_input("1.2.3"), Validation::Valid));
	assert!(matches!(
		validate_semver_input("not-a-version"),
		Validation::Invalid(_)
	));
	assert!(matches!(
		validate_reason_input("ship it"),
		Validation::Valid
	));
	assert!(matches!(
		validate_reason_input("   "),
		Validation::Invalid(_)
	));
	assert_eq!(normalize_optional_text(String::new()), None);
	assert_eq!(normalize_optional_text("   ".to_string()), None);
	assert_eq!(
		normalize_optional_text("details".to_string()),
		Some("details".to_string())
	);
}

#[test]
fn build_selectable_targets_lists_groups_then_standalone_then_group_members() {
	let configuration = load_workspace_configuration(&fixture_path("monochange/release-base"))
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let targets = build_selectable_targets(&configuration);
	let displays = targets
		.iter()
		.map(|target| target.display.clone())
		.collect::<Vec<_>>();
	assert_eq!(
		displays,
		vec![
			"[group] sdk (core, app)".to_string(),
			"[package] app (member of group `sdk`)".to_string(),
			"[package] core (member of group `sdk`)".to_string(),
		]
	);
}

#[test]
fn build_selectable_targets_collects_configured_change_types_per_target() {
	let configuration = load_workspace_configuration(&fixture_path(
		"changeset-target-metadata/cli-type-only-change",
	))
	.unwrap_or_else(|error| panic!("configuration: {error}"));
	let targets = build_selectable_targets(&configuration);
	let core = targets
		.iter()
		.find(|target| target.id == "core")
		.unwrap_or_else(|| panic!("expected core target"));
	assert_eq!(core.kind, TargetKind::Package);
	// Declared types inherit the built-in vocabulary, so the prompt offers the
	// merged, sorted set.
	assert_eq!(
		core.configured_types,
		vec![
			"breaking".to_string(),
			"change".to_string(),
			"docs".to_string(),
			"feat".to_string(),
			"fix".to_string(),
			"major".to_string(),
			"minor".to_string(),
			"none".to_string(),
			"patch".to_string(),
			"refactor".to_string(),
			"security".to_string(),
			"test".to_string(),
		]
	);
}

#[test]
fn run_interactive_change_returns_cancelled_variant_is_distinct_from_config_error() {
	let config_error =
		run_interactive_change(&empty_configuration(), &InteractiveOptions::default())
			.err()
			.unwrap_or_else(|| panic!("expected error"));
	assert!(
		matches!(config_error, MonochangeError::Config(..)),
		"empty workspace should produce Config error, not Cancelled"
	);
	assert!(
		!matches!(config_error, MonochangeError::Cancelled),
		"empty workspace should not produce Cancelled"
	);
}
