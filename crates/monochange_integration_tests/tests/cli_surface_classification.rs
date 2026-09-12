//! Integration tests for registered package CLI command-surface classification.

use std::ffi::OsString;
use std::path::Path;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git;
use monochange_test_helpers::snapshot_settings;
use serde_json::Value;
use tempfile::TempDir;
use tempfile::tempdir;

/// Copy the fixture, commit the `before` state as the base, then apply the
/// `after` state as the pull request head commit. The committed baseline under
/// `.monochange/cli-snapshots/` still describes the released surface, exactly
/// like a feature pull request.
fn setup_cli_fixture() -> TempDir {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cli-snapshot-registration");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	copy_directory(&fixture_root.join("before"), tempdir.path());
	git(tempdir.path(), &["init", "--initial-branch", "main"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);

	copy_directory(&fixture_root.join("after"), tempdir.path());
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "cli changes"]);

	tempdir
}

fn run_mc(root: &Path, args: &[&str]) -> String {
	let mut cli_args = vec![OsString::from("monochange")];
	cli_args.extend(args.iter().map(OsString::from));

	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));

	runtime
		.block_on(monochange::run_with_args_in_dir(
			"monochange",
			cli_args,
			root,
		))
		.unwrap_or_else(|error| panic!("monochange {}: {error}", args.join(" ")))
}

fn run_json(root: &Path, args: &[&str]) -> Value {
	let output = run_mc(root, args);
	serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("parse json output: {error}\n{output}"))
}

fn package<'a>(report: &'a Value, package_id: &str) -> &'a Value {
	report["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages should be an array: {report:#}"))
		.iter()
		.find(|package| package["packageId"] == package_id)
		.unwrap_or_else(|| panic!("missing package {package_id}: {report:#}"))
}

fn finding_with_id<'a>(package: &'a Value, id: &str) -> &'a Value {
	package["findings"]
		.as_array()
		.unwrap_or_else(|| panic!("findings should be an array: {package:#}"))
		.iter()
		.find(|finding| finding["id"] == id)
		.unwrap_or_else(|| panic!("missing finding {id}: {package:#}"))
}

#[test]
fn change_classify_reports_cli_surface_breaks_for_registered_clis() {
	let fixture = setup_cli_fixture();

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	let demo = package(&report, "demo");
	assert_eq!(demo["recommendation"], "major");
	assert_eq!(demo["decision"]["proposedChangesetBump"], "major");
	assert_eq!(demo["decision"]["enforceableMinimum"], "major");
	assert_eq!(demo["decision"]["compatibilityImpact"], "breaking");
	assert_eq!(demo["decision"]["reviewRequired"], false);

	assert_eq!(
		demo["cli"]["name"], "demo",
		"cli identity should flow into the report: {demo:#}"
	);
	assert_eq!(demo["cli"]["status"], "diffed");
	assert_eq!(demo["cli"]["recommendation"], "major");
	assert_eq!(demo["cli"]["findingCount"], 2);
	assert_eq!(
		demo["cli"]["baseline"],
		".monochange/cli-snapshots/demo.json"
	);

	let removed = finding_with_id(demo, "monochange/cli-surface/option-removed/check");
	assert_eq!(removed["surface"], "cli");
	assert_eq!(removed["impact"], "breaking");
	assert_eq!(removed["bump"], "major");
	assert_eq!(removed["confidence"], "high");
	assert_eq!(removed["analyzer"]["id"], "monochange/cli-surface");
	assert_eq!(removed["location"], "check");
	assert_eq!(
		removed["summary"],
		"option `--format` was removed from `check`"
	);

	let added = finding_with_id(demo, "monochange/cli-surface/command-added/migrate");
	assert_eq!(added["impact"], "additive");
	assert_eq!(added["bump"], "minor");

	// A concrete cli-surface finding suppresses the unclassified fallback.
	assert!(
		!demo["findings"]
			.as_array()
			.unwrap_or_else(|| panic!("findings array: {demo:#}"))
			.iter()
			.any(|finding| finding["ruleId"] == "monochange/unclassified-source"),
		"cli findings should replace the unclassified fallback: {demo:#}"
	);

	assert_eq!(report["recommendation"], "major");
	assert_eq!(report["schemaVersion"], 4);

	snapshot_settings().bind(|| {
		assert_json_snapshot!(report);
	});
}

#[test]
fn change_classify_text_reports_describe_the_cli_comparison() {
	let fixture = setup_cli_fixture();

	let markdown = run_mc(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "markdown",
		],
	);

	assert!(
		markdown.contains("- CLI: `demo` (diffed against `.monochange/cli-snapshots/demo.json`, recommendation `major`, 2 finding(s))"),
		"{markdown}"
	);

	let text = run_mc(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "text",
		],
	);

	assert!(
		text.contains("  CLI: demo (diffed against `.monochange/cli-snapshots/demo.json`, recommendation `major`, 2 finding(s))"),
		"{text}"
	);
}

#[test]
fn change_classify_reports_missing_cli_baselines_without_failing() {
	let fixture = setup_cli_fixture();
	std::fs::remove_file(fixture.path().join(".monochange/cli-snapshots/demo.json"))
		.unwrap_or_else(|error| panic!("remove baseline: {error}"));

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	let demo = package(&report, "demo");
	assert_eq!(demo["cli"]["status"], "missing_baseline");
	assert_eq!(demo["cli"]["recommendation"], Value::Null);
	assert_eq!(demo["recommendation"], "patch");
	assert_eq!(demo["decision"]["reviewRequired"], true);
	assert!(
		demo["findings"]
			.as_array()
			.unwrap_or_else(|| panic!("findings array: {demo:#}"))
			.iter()
			.any(|finding| finding["ruleId"] == "monochange/unclassified-source"),
		"a missing baseline keeps the conservative unclassified fallback: {demo:#}"
	);
	assert!(
		report["warnings"]
			.as_array()
			.unwrap_or_else(|| panic!("warnings array: {report:#}"))
			.iter()
			.any(|warning| warning
				== "cli snapshot for `demo` was not compared: no committed baseline; run `monochange snapshot --save` for this package during release"),
		"{report:#}"
	);
}

#[test]
fn change_classify_reports_stale_cli_baselines_without_diffing() {
	let fixture = setup_cli_fixture();
	let baseline_path = fixture.path().join(".monochange/cli-snapshots/demo.json");
	let baseline = std::fs::read_to_string(&baseline_path)
		.unwrap_or_else(|error| panic!("read baseline: {error}"));
	std::fs::write(
		&baseline_path,
		baseline.replace("\"schema_version\": \"0.1\"", "\"schema_version\": \"0.0\""),
	)
	.unwrap_or_else(|error| panic!("write baseline: {error}"));

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	let demo = package(&report, "demo");
	assert_eq!(demo["cli"]["status"], "stale_baseline");
	assert_eq!(demo["recommendation"], "patch");
	assert!(
		report["warnings"]
			.as_array()
			.unwrap_or_else(|| panic!("warnings array: {report:#}"))
			.iter()
			.any(|warning| {
				let text = warning.as_str().unwrap_or_default();
				text.contains("cli snapshot for `demo` was not compared")
					&& text.contains("schema version `0.0`")
			}),
		"{report:#}"
	);
}

#[test]
fn change_classify_reports_failed_cli_snapshot_capture() {
	let fixture = setup_cli_fixture();
	let config_path = fixture.path().join("monochange.toml");
	let config = std::fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read config: {error}"));
	std::fs::write(
		&config_path,
		config.replace("cat cli.json", "cat missing-snapshot.json"),
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	let demo = package(&report, "demo");
	assert_eq!(demo["cli"]["status"], "failed");
	assert_eq!(demo["recommendation"], "patch");
	assert_eq!(demo["decision"]["reviewRequired"], true);
	assert!(
		report["warnings"]
			.as_array()
			.unwrap_or_else(|| panic!("warnings array: {report:#}"))
			.iter()
			.any(|warning| {
				warning
					.as_str()
					.unwrap_or_default()
					.contains("cli snapshot for `demo` failed")
			}),
		"{report:#}"
	);
}

#[test]
fn change_classify_skips_cli_snapshots_when_asked() {
	let fixture = setup_cli_fixture();

	let report = run_json(
		fixture.path(),
		&[
			"change",
			"classify",
			"--base",
			"HEAD~1",
			"--head",
			"HEAD",
			"--skip-cli-snapshots",
			"--format",
			"json",
		],
	);

	let demo = package(&report, "demo");
	assert_eq!(demo["cli"]["status"], "skipped");
	assert_eq!(demo["cli"]["recommendation"], Value::Null);
	assert_eq!(demo["recommendation"], "patch");
	assert!(
		demo["findings"]
			.as_array()
			.unwrap_or_else(|| panic!("findings array: {demo:#}"))
			.iter()
			.all(|finding| finding["surface"] != "cli"),
		"skipped comparisons must not contribute cli findings: {demo:#}"
	);
}

#[test]
fn snapshot_command_lists_registers_and_saves_committed_baselines() {
	let fixture = setup_cli_fixture();
	// Corrupt the committed baseline so the first listing reports it as stale.
	let baseline_path = fixture.path().join(".monochange/cli-snapshots/demo.json");
	let baseline = std::fs::read_to_string(&baseline_path)
		.unwrap_or_else(|error| panic!("read baseline: {error}"));
	std::fs::write(
		&baseline_path,
		baseline.replace("\"schema_version\": \"0.1\"", "\"schema_version\": \"0.0\""),
	)
	.unwrap_or_else(|error| panic!("write baseline: {error}"));

	let listing = run_mc(fixture.path(), &["snapshot", "--list"]);
	assert!(
		listing.contains("name\tpackage\tsnapshot command\tbaseline"),
		"{listing}"
	);
	assert!(
		listing.contains("demo\tdemo\tcat cli.json\tbaseline stale or invalid"),
		"{listing}"
	);

	let captured = run_mc(fixture.path(), &["snapshot", "--package", "demo"]);
	let value: Value = serde_json::from_str(&captured)
		.unwrap_or_else(|error| panic!("captured snapshot was not JSON: {error}\n{captured}"));
	assert_eq!(value["tool"]["name"], "demo");

	let saved = run_mc(fixture.path(), &["snapshot", "--package", "demo", "--save"]);
	assert_eq!(
		saved,
		"Saved cli snapshot for `demo` to .monochange/cli-snapshots/demo.json"
	);

	let listing = run_mc(fixture.path(), &["snapshot", "--list"]);
	assert!(
		listing.contains("demo\tdemo\tcat cli.json\tbaseline current"),
		"{listing}"
	);
}

#[test]
fn snapshot_command_rejects_unknown_and_unregistered_packages() {
	let fixture = setup_cli_fixture();

	let unknown = run_mc_capture_error(fixture.path(), &["snapshot", "--package", "ghost"]);
	assert!(unknown.contains("unknown package `ghost`"), "{unknown}");

	let unregistered = run_mc_capture_error(fixture.path(), &["snapshot", "--package", "plain"]);
	assert!(
		unregistered.contains("package `plain` does not register a cli"),
		"{unregistered}"
	);
}

fn run_mc_capture_error(root: &Path, args: &[&str]) -> String {
	let mut cli_args = vec![OsString::from("monochange")];
	cli_args.extend(args.iter().map(OsString::from));

	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap_or_else(|error| panic!("tokio runtime: {error}"));

	let error = runtime
		.block_on(async { monochange::run_with_args_in_dir("monochange", cli_args, root).await })
		.expect_err("expected the command to fail");
	error.to_string()
}
