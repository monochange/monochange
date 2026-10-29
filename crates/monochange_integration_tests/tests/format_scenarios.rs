use std::fs;
use std::path::PathBuf;

use monochange_test_helpers::snapshot_settings;
use rstest::rstest;

/// Format every checked-in scenario twice: once for the snapshot and once to
/// prove the output is stable on a second pass.
#[rstest]
fn formats_each_fixture_scenario(
	#[files("../../fixtures/tests/monochange/format-scenarios/*/monochange.toml")] path: PathBuf,
) {
	let scenario = path
		.parent()
		.and_then(|parent| parent.file_name())
		.and_then(|name| name.to_str())
		.unwrap_or("scenario")
		.to_string();
	let source =
		fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {scenario}: {error}"));
	let formatted = monochange_config::format_workspace_configuration(&source)
		.unwrap_or_else(|error| panic!("format {scenario}: {error}"));
	let second = monochange_config::format_workspace_configuration(&formatted)
		.unwrap_or_else(|error| panic!("reformat {scenario}: {error}"));

	assert_eq!(
		formatted, second,
		"formatting must be idempotent for {scenario}"
	);

	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(scenario);
	settings.bind(|| insta::assert_snapshot!(formatted));
}
