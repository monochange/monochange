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
