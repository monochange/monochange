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
		["leptoskit_windows", "gallery", "leptoskit_plugin_*"]
	);
	assert_eq!(
		settings.jobs["test-windows"].paths,
		["crates/leptoskit_windows/android/**"]
	);
	assert!(!settings.jobs["test-windows"].always);
	assert!(settings.jobs["docs"].always);
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
	assert_eq!(settings.jobs["lint"].packages, ["core"]);
	assert!(settings.jobs["lint"].paths.is_empty());
	assert!(!settings.jobs["lint"].always);
}
