//! Integration tests for API change classification commands.

use std::ffi::OsString;
use std::path::Path;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git;
use monochange_test_helpers::snapshot_settings;
use serde_json::Value;
use tempfile::TempDir;
use tempfile::tempdir;

fn setup_api_fixture(name: &str) -> TempDir {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/api-classification")
		.join(name);
	let before = fixture_root.join("before");
	let after = fixture_root.join("after");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	copy_directory(&before, tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);

	copy_directory(&after, tempdir.path());
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "api changes"]);

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

fn assert_package_recommendation(report: &Value, package_id: &str, expected: &str) {
	let package = package(report, package_id);
	assert_eq!(package["recommendation"], expected);
	assert_eq!(package["decision"]["proposedChangesetBump"], expected);
	assert!(
		package["findings"]
			.as_array()
			.is_some_and(|changes| !changes.is_empty()),
		"expected classification findings for {package_id}: {package:#}"
	);
}

#[test]
fn change_classify_detects_rust_typescript_and_javascript_api_impacts() {
	let fixture = setup_api_fixture("mixed-api");

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	assert_eq!(report["recommendation"], "major");
	assert_eq!(report["schemaVersion"], 1);
	assert_package_recommendation(&report, "rust_core", "major");
	assert_package_recommendation(&report, "ts_client", "minor");
	assert_package_recommendation(&report, "js_utils", "patch");
	assert!(report["comparisons"].as_array().is_some_and(|comparisons| {
		comparisons
			.iter()
			.any(|comparison| comparison["kind"] == "pullRequest")
	}));

	snapshot_settings().bind(|| {
		assert_json_snapshot!(report);
	});
}

#[test]
fn api_diff_uses_the_same_classifier_for_mixed_api_impacts() {
	let fixture = setup_api_fixture("mixed-api");

	let report = run_json(
		fixture.path(),
		&[
			"api", "diff", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	assert_eq!(report["recommendation"], "major");
	assert_package_recommendation(&report, "rust_core", "major");
	assert_package_recommendation(&report, "ts_client", "minor");
	assert_package_recommendation(&report, "js_utils", "patch");
}

#[test]
fn change_classify_uses_the_net_candidate_when_local_edits_revert_the_branch() {
	let fixture = setup_api_fixture("net-revert-changeset");
	let before = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/api-classification/net-revert-changeset/before");
	copy_directory(&before, fixture.path());

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	assert_eq!(report["recommendation"], "none");
	let package = package(&report, "core");
	assert_eq!(package["recommendation"], "none");
	assert_eq!(package["action"], "review");
	assert_eq!(package["decision"]["reviewRequired"], true);
	assert_eq!(package["existingChangesets"][0]["bump"], "major");
	assert!(report["comparisons"].as_array().is_some_and(|comparisons| {
		comparisons.iter().any(|comparison| {
			comparison["kind"] == "workingTree"
				&& comparison["note"]
					.as_str()
					.is_some_and(|note| note.contains("final candidate"))
		})
	}));
}

#[test]
fn change_classify_supports_global_jq_and_equals_options() {
	let fixture = setup_api_fixture("mixed-api");

	let output = run_mc(
		fixture.path(),
		&[
			"--jq",
			".schemaVersion",
			"change",
			"classify",
			"--base=HEAD~1",
			"--head=HEAD",
			"--format=json",
		],
	);

	assert_eq!(output, "1");
}

#[test]
fn changeset_api_validation_writes_the_requested_report() {
	let fixture = setup_api_fixture("stale-changeset");
	let report_path = fixture.path().join("classification.json");
	let output_arg = format!("--output={}", report_path.display());

	let output = run_mc(
		fixture.path(),
		&[
			"changeset",
			"validate",
			"--api",
			"--base=HEAD~1",
			"--head=HEAD",
			"--format=json",
			&output_arg,
		],
	);
	let written = std::fs::read_to_string(&report_path)
		.unwrap_or_else(|error| panic!("read {}: {error}", report_path.display()));

	assert_eq!(written, output);
	assert_eq!(
		serde_json::from_str::<Value>(&written)
			.unwrap_or_else(|error| panic!("parse written report: {error}"))["schemaVersion"],
		1
	);
}

#[test]
fn changeset_api_validation_writes_the_complete_markdown_output() {
	let fixture = setup_api_fixture("stale-changeset");
	let report_path = fixture.path().join("classification.md");
	let output_arg = format!("--output={}", report_path.display());

	let output = run_mc(
		fixture.path(),
		&[
			"changeset",
			"validate",
			"--api",
			"--base=HEAD~1",
			"--head=HEAD",
			"--format=markdown",
			&output_arg,
		],
	);
	let written = std::fs::read_to_string(&report_path)
		.unwrap_or_else(|error| panic!("read {}: {error}", report_path.display()));

	assert_eq!(written, output);
	assert!(written.starts_with("# Changeset API validation"));
}

#[test]
fn affected_changeset_policy_snapshots_understated_api_bump_output() {
	let fixture = setup_api_fixture("changeset-bump-alignment");

	let evaluation = run_json(
		fixture.path(),
		&[
			"step",
			"affected-packages",
			"--from",
			"HEAD~1",
			"--format",
			"json",
		],
	);

	assert_eq!(evaluation["status"], "failed");
	assert_eq!(
		evaluation["covered_package_ids"],
		serde_json::json!(["core"])
	);
	assert!(evaluation["errors"].as_array().is_some_and(|errors| {
		errors.iter().any(|error| {
			error.as_str().is_some_and(|error| {
				error.contains("requested `patch`") && error.contains("recommends `major`")
			})
		})
	}));

	snapshot_settings().bind(|| {
		assert_json_snapshot!(evaluation);
	});
}

#[test]
fn change_classify_detects_dart_api_impacts() {
	let fixture = setup_api_fixture("dart-api");

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	assert_eq!(report["recommendation"], "minor");
	assert_package_recommendation(&report, "mobile", "minor");
}

#[test]
fn change_classify_reports_changeset_only_intent_for_review() {
	let fixture = setup_api_fixture("stale-changeset");

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);
	let package = package(&report, "core");

	assert_eq!(package["recommendation"], "none");
	assert_eq!(package["action"], "review");
	assert_eq!(package["decision"]["compatibilityImpact"], "unknown");
	assert_eq!(package["decision"]["completeness"], "unsupported");
	assert_eq!(package["decision"]["reviewRequired"], true);
	assert_eq!(package["existingChangesets"][0]["bump"], "minor");
}
