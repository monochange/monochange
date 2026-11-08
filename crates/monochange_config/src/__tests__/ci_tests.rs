//! Tests for the `[ci]` configuration module.

use std::fs;

use tempfile::tempdir;

use super::*;

/// Write `contents` as `monochange.toml` inside a fresh temp directory.
fn config_dir(contents: &str) -> tempfile::TempDir {
	let dir = tempdir().unwrap();

	fs::write(dir.path().join("monochange.toml"), contents).unwrap();
	dir
}

#[test]
fn load_ci_settings_returns_defaults_when_the_file_is_missing() {
	let dir = tempdir().unwrap();
	let settings = load_ci_settings(dir.path()).unwrap();

	assert_eq!(settings.provider, "graph");
	assert_eq!(settings.model, None);
	assert!(!settings.ai_can_skip);
	assert_eq!(settings.max_skips_on_main, 5);
	assert_eq!(
		settings.unconditional_events,
		["schedule", "workflow_dispatch"]
	);
	assert_eq!(
		settings.unconditional_branches,
		["release/*", "monochange/release/*", "refs/tags/v*"]
	);
	assert_eq!(
		settings.unconditional_paths,
		["Cargo.lock", "rust-toolchain.toml", ".github/workflows/**"]
	);
	assert_eq!(settings.ignore_paths, ["**/*.md", "docs/**"]);
	assert!(settings.jobs.is_empty());
}

#[test]
fn load_ci_settings_parses_jobs_and_overrides() {
	let dir = config_dir(
		r#"
[ci]
provider = "opencode-go"
model = "deepseek-v4.1"
max_skips_on_main = 2

[ci.jobs.test-windows]
packages = ["leptoskit_windows", "gallery", "leptoskit_plugin_*"]
paths = ["crates/leptoskit_windows/android/**"]
always = false

[ci.jobs.docs]
always = true
"#,
	);

	let settings = load_ci_settings(dir.path()).unwrap();

	assert_eq!(settings.provider, "opencode-go");
	assert_eq!(settings.model.as_deref(), Some("deepseek-v4.1"));
	assert!(!settings.ai_can_skip);
	assert_eq!(settings.max_skips_on_main, 2);
	assert_eq!(
		settings.jobs["test-windows"].packages,
		[
			CiJobPackage {
				name: "leptoskit_windows".to_string(),
				selection: FeatureSelection::AllFeatures,
			},
			CiJobPackage {
				name: "gallery".to_string(),
				selection: FeatureSelection::AllFeatures,
			},
			CiJobPackage {
				name: "leptoskit_plugin_*".to_string(),
				selection: FeatureSelection::AllFeatures,
			},
		]
	);
	assert_eq!(
		settings.jobs["test-windows"].paths,
		["crates/leptoskit_windows/android/**"]
	);
	assert!(!settings.jobs["test-windows"].always);
	assert!(settings.jobs["docs"].always);
}

#[test]
fn load_ci_settings_parses_feature_selection_tables() {
	let dir = config_dir(
		r#"
[ci.jobs.nightly]
packages = [
	"leptoskit_rpc",
	{ name = "leptoskit", features = ["nightly", "leptos"] },
	{ name = "leptoskit_use", default_features = false },
	{ name = "leptoskit", all_features = true },
	{ name = "leptoskit_query", features = ["nightly"], default_features = false },
	{ name = "leptoskit_macro" },
]
"#,
	);

	let settings = load_ci_settings(dir.path()).unwrap();

	assert_eq!(
		settings.jobs["nightly"].packages,
		[
			CiJobPackage {
				name: "leptoskit_rpc".to_string(),
				selection: FeatureSelection::AllFeatures,
			},
			CiJobPackage {
				name: "leptoskit".to_string(),
				selection: FeatureSelection::DefaultsPlus(vec![
					"nightly".to_string(),
					"leptos".to_string()
				]),
			},
			CiJobPackage {
				name: "leptoskit_use".to_string(),
				selection: FeatureSelection::Only(Vec::new()),
			},
			CiJobPackage {
				name: "leptoskit".to_string(),
				selection: FeatureSelection::AllFeatures,
			},
			CiJobPackage {
				name: "leptoskit_query".to_string(),
				selection: FeatureSelection::Only(vec!["nightly".to_string()]),
			},
			CiJobPackage {
				name: "leptoskit_macro".to_string(),
				selection: FeatureSelection::DefaultsPlus(Vec::new()),
			},
		]
	);
}

#[test]
fn load_ci_settings_rejects_contradictory_feature_selections() {
	let all_features_and_features = config_dir(
		"[ci.jobs.nightly]\npackages = [{ name = \"leptoskit\", all_features = true, features = [\"nightly\"] }]\n",
	);
	let error = load_ci_settings(all_features_and_features.path()).unwrap_err();

	assert!(
		error.to_string().contains(
			"job `nightly` package `leptoskit` sets `all_features = true` together with `features` or `default_features`"
		),
		"unexpected error: {error}"
	);

	let all_features_and_defaults = config_dir(
		"[ci.jobs.nightly]\npackages = [{ name = \"leptoskit\", all_features = true, default_features = true }]\n",
	);
	let error = load_ci_settings(all_features_and_defaults.path()).unwrap_err();

	assert!(
		error.to_string().contains(
			"job `nightly` package `leptoskit` sets `all_features = true` together with `features` or `default_features`"
		),
		"unexpected error: {error}"
	);

	let glob_with_features = config_dir(
		"[ci.jobs.nightly]\npackages = [{ name = \"leptoskit_plugin_*\", features = [\"nightly\"] }]\n",
	);
	let error = load_ci_settings(glob_with_features.path()).unwrap_err();

	assert!(
		error.to_string().contains(
			"job `nightly` package `leptoskit_plugin_*` combines a glob with a feature selection"
		),
		"unexpected error: {error}"
	);

	let unknown_table_field =
		config_dir("[ci.jobs.nightly]\npackages = [{ name = \"leptoskit\", extra = true }]\n");
	let error = load_ci_settings(unknown_table_field.path()).unwrap_err();

	assert!(
		error.to_string().contains("unknown field"),
		"unexpected error: {error}"
	);

	let wrong_entry_type = config_dir("[ci.jobs.nightly]\npackages = [1]\n");
	let error = load_ci_settings(wrong_entry_type.path()).unwrap_err();

	assert!(
		error
			.to_string()
			.contains("a package name string or a feature-selection table"),
		"unexpected error: {error}"
	);
}

#[test]
fn job_package_labels_spell_out_the_feature_selection() {
	let cases = [
		(
			CiJobPackage {
				name: "leptoskit".to_string(),
				selection: FeatureSelection::AllFeatures,
			},
			"leptoskit",
		),
		(
			CiJobPackage {
				name: "leptoskit".to_string(),
				selection: FeatureSelection::DefaultsPlus(Vec::new()),
			},
			"leptoskit (default features)",
		),
		(
			CiJobPackage {
				name: "leptoskit".to_string(),
				selection: FeatureSelection::DefaultsPlus(vec![
					"nightly".to_string(),
					"leptos".to_string(),
				]),
			},
			"leptoskit (default + nightly,leptos)",
		),
		(
			CiJobPackage {
				name: "leptoskit_use".to_string(),
				selection: FeatureSelection::Only(Vec::new()),
			},
			"leptoskit_use (no default features)",
		),
		(
			CiJobPackage {
				name: "leptoskit_rpc".to_string(),
				selection: FeatureSelection::Only(vec!["nightly".to_string()]),
			},
			"leptoskit_rpc (nightly)",
		),
	];

	for (package, expected) in cases {
		assert_eq!(package.label(), expected);
	}
}

#[test]
fn load_ci_settings_rejects_unknown_providers_and_fields() {
	let unknown_provider = config_dir("[ci]\nprovider = \"not-a-provider\"\n");
	let error = load_ci_settings(unknown_provider.path()).unwrap_err();

	assert!(
		error
			.to_string()
			.contains("unsupported `[ci]` provider `not-a-provider`"),
		"unexpected error: {error}"
	);

	let unknown_field = config_dir("[ci]\nenabled = true\n");
	let error = load_ci_settings(unknown_field.path()).unwrap_err();

	assert!(
		error.to_string().contains("unknown field"),
		"unexpected error: {error}"
	);
}

#[test]
fn raw_defaults_apply_when_keys_are_omitted() {
	let dir = config_dir("[ci.jobs.lint]\npackages = [\"core\"]\n");
	let settings = load_ci_settings(dir.path()).unwrap();

	assert_eq!(settings.provider, "graph");
	assert!(!settings.ai_can_skip);
	assert_eq!(settings.max_skips_on_main, DEFAULT_MAX_SKIPS_ON_MAIN);
	assert_eq!(
		settings.unconditional_events,
		DEFAULT_UNCONDITIONAL_EVENTS
			.iter()
			.map(|value| (*value).to_string())
			.collect::<Vec<_>>()
	);
	assert_eq!(
		settings.jobs["lint"].packages,
		[CiJobPackage {
			name: "core".to_string(),
			selection: FeatureSelection::AllFeatures,
		}]
	);
	assert!(settings.jobs["lint"].paths.is_empty());
	assert!(!settings.jobs["lint"].always);
}
