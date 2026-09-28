#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]
use rstest::rstest;

mod test_support;
use test_support::assert_readable_json_snapshot;
use test_support::current_test_name;
use test_support::monochange_command;
use test_support::run_json_command;
use test_support::setup_scenario_workspace;
use test_support::snapshot_settings;

#[rstest]
#[case::group("group")]
#[case::ungrouped("ungrouped")]
fn open_release_pull_request_dry_run_matches_snapshot(#[case] scenario: &str) {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let scenario_relative = format!("release-pr/{scenario}");
	let tempdir = setup_scenario_workspace(&scenario_relative);
	let json = run_json_command(tempdir.path(), "release-pr", Some("2026-04-06"));
	assert_readable_json_snapshot!(json);
}

#[test]
fn release_pull_request_body_is_capped_at_the_configured_limit() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("release-pr/capped-body");
	let json = run_json_command(tempdir.path(), "release-pr", Some("2026-04-06"));
	let request = json
		.get("release_request")
		.unwrap_or_else(|| panic!("expected a release request in the command result"));
	let truncation = request
		.get("body_truncation")
		.unwrap_or_else(|| panic!("expected the configured cap to shorten the body"));
	assert_eq!(truncation["max_chars"], 500);
	assert!(
		truncation["original_chars"].as_u64().unwrap_or_default() > 500,
		"the untruncated notes exceed the cap"
	);
	assert_eq!(truncation["dropped_entries"], 3);
	let body = request
		.get("body")
		.and_then(serde_json::Value::as_str)
		.unwrap_or_else(|| panic!("expected a rendered body"));
	assert!(body.chars().count() <= 500);
	assert!(body.contains("## Prepared release"));
	assert!(body.contains("## Full release notes"));
	assert!(body.contains("crates/core/CHANGELOG.md"));
	insta::assert_snapshot!(
		"release_pull_request_body_is_capped_at_the_configured_limit__body",
		body
	);
}

#[test]
fn release_pull_request_body_truncation_is_reported_in_markdown_output() {
	let tempdir = setup_scenario_workspace("release-pr/capped-body");
	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.args(["run", "release-pr", "--dry-run", "--format", "md"])
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(
		stdout.contains("## Release request warnings"),
		"the markdown result reports the shortened body:\n{stdout}"
	);
	assert!(stdout.contains("release request body shortened to 500 characters (from "));
}

#[test]
fn release_pull_request_body_truncation_is_reported_in_text_output() {
	let tempdir = setup_scenario_workspace("release-pr/capped-body");
	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.args(["run", "release-pr", "--dry-run"])
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stdout = String::from_utf8_lossy(&output.stdout);
	assert!(
		stdout.contains("release request warnings:"),
		"the step reports the shortened body before the next create call:\n{stdout}"
	);
	assert!(stdout.contains("release request body shortened to 500 characters (from "));
	assert!(stdout.contains("[source.pull_requests].max_body_chars"));
}
