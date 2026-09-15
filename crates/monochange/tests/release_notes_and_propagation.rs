#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]
use std::fs;

use insta::assert_snapshot;
use rstest::rstest;
use serde_json::Value;

mod test_support;
use test_support::assert_readable_json_snapshot;
use test_support::current_test_name;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::ungrouped_patch("ungrouped-patch")]
#[case::ungrouped_caused_by("ungrouped-caused-by")]
#[case::grouped_default("grouped-default")]
fn release_note_changelog_snapshots_match_expected_output(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace(&format!("release-notes-and-propagation/{scenario}"));
	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("run")
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	match scenario {
		"ungrouped-patch" | "ungrouped-caused-by" => {
			let core_changelog =
				fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
					.unwrap_or_else(|error| panic!("core changelog: {error}"));
			let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
				.unwrap_or_else(|error| panic!("app changelog: {error}"));
			assert_snapshot!("core", core_changelog);
			assert_snapshot!("app", app_changelog);
		}
		"grouped-default" => {
			let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
				.unwrap_or_else(|error| panic!("app changelog: {error}"));
			let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
				.unwrap_or_else(|error| panic!("group changelog: {error}"));
			assert_snapshot!("app", app_changelog);
			assert_snapshot!("group", group_changelog);
		}
		_ => unreachable!("unexpected scenario"),
	}
}

#[test]
fn ungrouped_transitive_bump_with_parent_bump_minor_escalates_dependent_version() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix("app_decision");
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("release-notes-and-propagation/ungrouped-minor");
	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("run")
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json = serde_json::from_slice::<Value>(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));
	let app_decision = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.find(|decision| decision["package"].as_str() == Some("cargo:crates/app/Cargo.toml"))
		.unwrap_or_else(|| panic!("expected app decision"));

	assert_readable_json_snapshot!(app_decision);
}

#[test]
fn custom_empty_update_message_on_package_overrides_default() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir =
		setup_scenario_workspace("release-notes-and-propagation/ungrouped-custom-package-message");
	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("run")
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("package changelog: {error}"));
	assert_snapshot!(changelog);
}

/// Run the release for the shared-note scenario and return the tempdir plus
/// the group and package changelogs it wrote.
fn run_multi_target_shared_note() -> (tempfile::TempDir, String, String) {
	let tempdir =
		setup_scenario_workspace("release-notes-and-propagation/multi-target-shared-note");
	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("run")
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	(tempdir, group_changelog, core_changelog)
}

#[test]
fn one_changeset_renders_once_in_the_highest_priority_section() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let (_tempdir, group_changelog, _core_changelog) = run_multi_target_shared_note();
	assert_snapshot!(group_changelog);
}

#[test]
fn a_shared_changeset_entry_lists_every_affected_package() {
	let (_tempdir, group_changelog, _core_changelog) = run_multi_target_shared_note();

	// The one changeset targets core (major), app (minor), and cli (docs).
	// All three packages must survive in the breaking section, each with the
	// bump symbol for the bump it actually received.
	assert!(
		group_changelog.contains("🔴 _core_, 🟠 _app_, ⚪ _cli_"),
		"every package must be listed with its own bump symbol:\n{group_changelog}"
	);
	assert_eq!(
		group_changelog.matches("split floats and fixed").count(),
		1,
		"the shared change must appear exactly once:\n{group_changelog}"
	);
	assert!(
		!group_changelog.contains("### Features"),
		"the feature target must not create a second copy of the same change"
	);
	assert!(
		!group_changelog.contains("### Documentation"),
		"the docs target must not create a second copy of the same change"
	);
}

#[test]
fn a_package_changelog_omits_its_own_redundant_label() {
	let (_tempdir, _group_changelog, core_changelog) = run_multi_target_shared_note();

	assert!(
		core_changelog.contains("#### split floats and fixed"),
		"a targeted package must still receive the change:\n{core_changelog}"
	);
	assert!(
		!core_changelog.contains("_Packages:_"),
		"a package's own changelog must not repeat its name:\n{core_changelog}"
	);
}

#[test]
fn the_release_manifest_reports_one_entry_with_per_package_bumps() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let (tempdir, _group_changelog, _core_changelog) = run_multi_target_shared_note();

	// The fixture configures a `json` output, which is the format that keeps
	// entries structured instead of rendering them to Markdown strings.
	let notes = fs::read_to_string(tempdir.path().join("release-notes/sdk-2.0.0.json"))
		.unwrap_or_else(|error| panic!("json release notes: {error}"));
	let notes = serde_json::from_str::<Value>(&notes)
		.unwrap_or_else(|error| panic!("parse release notes json: {error}"));

	let sections = notes["sections"]
		.as_array()
		.unwrap_or_else(|| panic!("sections array"));
	let entries = sections
		.iter()
		.flat_map(|section| {
			section["entries"]
				.as_array()
				.unwrap_or_else(|| panic!("entries array"))
				.iter()
		})
		.collect::<Vec<_>>();
	assert_eq!(entries.len(), 1, "one change must produce one entry");

	let packages = entries[0]["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages array"));
	let bumps = packages
		.iter()
		.map(|package| {
			(
				package["name"].as_str().unwrap_or_default().to_string(),
				package["bump"].as_str().unwrap_or_default().to_string(),
			)
		})
		.collect::<Vec<_>>();
	assert_eq!(
		bumps,
		vec![
			("core".to_string(), "major".to_string()),
			("app".to_string(), "minor".to_string()),
			("cli".to_string(), "none".to_string()),
		],
		"the artifact must record each package's own bump"
	);
	assert_eq!(
		entries[0]["bump"].as_str(),
		Some("major"),
		"the merged entry keeps its highest severity"
	);
	assert_eq!(entries[0]["change_type"].as_str(), Some("breaking"));

	assert_readable_json_snapshot!(entries[0]);
}

#[test]
fn disabling_bump_symbols_restores_plain_package_labels() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir =
		setup_scenario_workspace("release-notes-and-propagation/multi-target-shared-note");
	// The fixture resolves defaults from this table, so appending to the
	// existing `[changelog]` section is enough to exercise the opt-out.
	let config_path = tempdir.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str("\n[changelog.style]\npackage_bump_symbols = false\n");
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("run")
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	assert_snapshot!(group_changelog);
}
