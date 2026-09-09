//! Integration tests for API change classification commands.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

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
	replace_host_target_placeholder(tempdir.path());
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

fn replace_host_target_placeholder(root: &Path) {
	let config_path = root.join("monochange.toml");
	let Ok(config) = std::fs::read_to_string(&config_path) else {
		return;
	};
	if !config.contains("__HOST_TARGET__") {
		return;
	}
	let output = Command::new("rustc")
		.arg("-vV")
		.output()
		.unwrap_or_else(|error| panic!("run rustc: {error}"));
	assert!(output.status.success(), "rustc -vV should succeed");
	let version = String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("rustc version should be UTF-8: {error}"));
	let host = version
		.lines()
		.find_map(|line| line.strip_prefix("host: "))
		.unwrap_or_else(|| panic!("rustc did not report its host target"));
	std::fs::write(&config_path, config.replace("__HOST_TARGET__", host))
		.unwrap_or_else(|error| panic!("write host target config: {error}"));
}

fn redact_host_target(value: &mut Value) {
	match value {
		Value::Array(values) => values.iter_mut().for_each(redact_host_target),
		Value::Object(object) => {
			if object.get("name").and_then(Value::as_str) == Some("host-target")
				&& let Some(Value::Object(configuration)) = object.get_mut("configuration")
			{
				configuration.insert(
					"target".to_string(),
					Value::String("[host target]".to_string()),
				);
			}
			object.values_mut().for_each(redact_host_target);
		}
		_ => {}
	}
}

fn setup_deleted_package_fixture() -> TempDir {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/api-classification/deleted-package");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	copy_directory(&fixture_root.join("before"), tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	git(tempdir.path(), &["tag", "retired/v1.0.0"]);
	git(tempdir.path(), &["tag", "retired_js/v2.0.0"]);

	std::fs::remove_dir_all(tempdir.path().join("crates/retired"))
		.unwrap_or_else(|error| panic!("remove retired package: {error}"));
	std::fs::remove_dir_all(tempdir.path().join("packages/retired-js"))
		.unwrap_or_else(|error| panic!("remove retired JavaScript package: {error}"));
	std::fs::remove_file(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("remove monochange config: {error}"));
	copy_directory(&fixture_root.join("after"), tempdir.path());
	git(tempdir.path(), &["add", "--all"]);
	git(tempdir.path(), &["commit", "-m", "delete package"]);

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
	assert_eq!(report["schemaVersion"], 3);
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
fn change_classify_detects_feature_gated_rust_breaks_in_the_configured_matrix() {
	let fixture = setup_api_fixture("rust-semver-matrix");
	let report = run_json(
		fixture.path(),
		&[
			"change",
			"classify",
			"--base",
			"HEAD~1",
			"--head",
			"HEAD",
			"--detection-level",
			"semantic",
			"--format",
			"json",
		],
	);
	let rust_api = package(&report, "rust_api");
	let matrix = rust_api["findings"]
		.as_array()
		.and_then(|findings| {
			findings.iter().find(|finding| {
				finding["analyzer"]["id"] == "cargo/cargo-semver-checks"
					&& finding["comparisons"]
						.as_array()
						.is_some_and(|comparisons| {
							comparisons
								.iter()
								.any(|comparison| comparison == "pullRequest")
						})
			})
		})
		.unwrap_or_else(|| panic!("missing Rust matrix finding: {rust_api:#}"));
	let checks = matrix["coverage"]["checks"]
		.as_array()
		.unwrap_or_else(|| panic!("matrix checks should be an array"));
	let bump_for = |name: &str| {
		checks
			.iter()
			.find(|check| check["name"] == name)
			.map(|check| check["suggestedBump"].clone())
	};

	assert_eq!(rust_api["recommendation"], "major");
	assert_eq!(bump_for("default"), Some(serde_json::json!("none")));
	assert_eq!(
		bump_for("no-default-features"),
		Some(serde_json::json!("none"))
	);
	assert_eq!(
		bump_for("selected-experimental"),
		Some(serde_json::json!("major"))
	);
	assert_eq!(bump_for("all-features"), Some(serde_json::json!("major")));
	assert_eq!(bump_for("host-target"), Some(serde_json::json!("major")));
	assert!(checks.iter().any(|check| {
		check["diagnostics"].as_array().is_some_and(|diagnostics| {
			diagnostics
				.iter()
				.any(|diagnostic| diagnostic["code"] == "trait_method_missing")
		})
	}));

	let mut snapshot = report.clone();
	redact_host_target(&mut snapshot);
	snapshot_settings().bind(|| {
		assert_json_snapshot!(snapshot);
	});
}

#[test]
fn change_classify_detects_a_package_deleted_from_the_candidate() {
	let fixture = setup_deleted_package_fixture();

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	assert_eq!(report["recommendation"], "major");
	assert_package_recommendation(&report, "retired", "major");
	assert_package_recommendation(&report, "retired_js", "major");
	let retired = package(&report, "retired");
	assert_eq!(retired["releaseOwner"]["id"], "retired");
	assert_eq!(retired["releaseOwner"]["latestRelease"], "retired/v1.0.0");
	assert!(retired["findings"].as_array().is_some_and(|findings| {
		let removed_public_api = findings.iter().any(|finding| {
			finding["change"] == "removed"
				&& finding["impact"] == "breaking"
				&& finding["surface"] == "public_api"
				&& finding["location"] == "src/lib.rs"
		});
		let removed_package = findings.iter().any(|finding| {
			finding["ruleId"] == "monochange/package-lifecycle/package/removed/package"
				&& finding["confidence"] == "high"
				&& finding["coverage"]["completeness"] == "complete"
				&& finding["location"] == "Cargo.toml"
		});
		removed_public_api && removed_package
	}));
	let retired_js = package(&report, "retired_js");
	assert_eq!(retired_js["releaseOwner"]["id"], "retired_js");
	assert_eq!(
		retired_js["releaseOwner"]["latestRelease"],
		"retired_js/v2.0.0"
	);
	assert!(retired_js["findings"].as_array().is_some_and(|findings| {
		let removed_export = findings.iter().any(|finding| {
			finding["change"] == "removed"
				&& finding["impact"] == "breaking"
				&& finding["location"] == "src/index.ts"
		});
		let removed_package = findings.iter().any(|finding| {
			finding["ruleId"] == "monochange/package-lifecycle/package/removed/package"
				&& finding["confidence"] == "high"
				&& finding["coverage"]["completeness"] == "complete"
				&& finding["location"] == "package.json"
		});
		removed_export && removed_package
	}));

	snapshot_settings().bind(|| {
		assert_json_snapshot!(report);
	});
}

#[test]
fn change_classify_compares_an_explicit_release_with_the_candidate_and_default_branch() {
	let fixture = setup_api_fixture("mixed-api");

	let report = run_json(
		fixture.path(),
		&[
			"change",
			"classify",
			"--base",
			"HEAD~1",
			"--head",
			"HEAD",
			"--release",
			"HEAD~1",
			"--format",
			"json",
		],
	);

	for package_id in ["rust_core", "ts_client", "js_utils"] {
		let comparisons = package(&report, package_id)["comparisons"]
			.as_array()
			.unwrap_or_else(|| panic!("package comparisons should be an array"));
		assert!(comparisons.iter().any(|comparison| {
			comparison["kind"] == "release"
				&& comparison["base"] == "HEAD~1"
				&& comparison["status"] == "analyzed"
		}));
		assert!(comparisons.iter().any(|comparison| {
			comparison["kind"] == "releaseToDefault" && comparison["base"] == "HEAD~1"
		}));
	}
}

#[test]
fn change_classify_discovers_the_default_branch_and_latest_release_tag() {
	let fixture = setup_api_fixture("mixed-api");
	git(fixture.path(), &["branch", "-M", "main"]);
	git(fixture.path(), &["tag", "v0.1.0", "HEAD~1"]);
	git(fixture.path(), &["tag", "rust_core/v0.1.0", "HEAD~1"]);

	let report = run_json(
		fixture.path(),
		&["change", "classify", "--head", "HEAD", "--format", "json"],
	);

	assert_eq!(report["defaultBranch"], "main");
	assert!(report["packages"].as_array().is_some_and(|packages| {
		packages
			.iter()
			.all(|package| package["releaseOwner"]["latestRelease"].is_string())
	}));
}

#[test]
fn change_classify_limits_the_report_to_selected_packages() {
	let fixture = setup_api_fixture("mixed-api");

	let report = run_json(
		fixture.path(),
		&[
			"change",
			"classify",
			"--base",
			"HEAD~1",
			"--package",
			"rust_core",
			"--format",
			"json",
		],
	);

	assert_eq!(report["packages"].as_array().map(Vec::len), Some(1));
	assert_eq!(report["packages"][0]["packageId"], "rust_core");
}

#[test]
fn change_classify_warns_when_a_pending_changeset_cannot_be_inspected() {
	let fixture = setup_api_fixture("stale-changeset");
	std::fs::write(
		fixture.path().join(".changeset/stale.md"),
		"not frontmatter\n",
	)
	.unwrap_or_else(|error| panic!("replace pending changeset: {error}"));

	let report = run_json(
		fixture.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);

	assert!(report["warnings"].as_array().is_some_and(|warnings| {
		warnings.iter().any(|warning| {
			warning
				.as_str()
				.is_some_and(|warning| warning.contains("could not inspect pending changeset"))
		})
	}));
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

	assert_eq!(output, "3");
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
		3
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
