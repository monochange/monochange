//! End-to-end regressions for Rust public-API classification (`cargo/public-api`)
//! and changeset bump alignment.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use insta::assert_json_snapshot;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::get_cargo_bin;
use monochange_test_helpers::git::git;
use serde_json::Value;
use tempfile::TempDir;

fn fixture_path(scenario: &str, stage: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo-api-classification")
		.join(scenario)
		.join(stage)
}

fn setup_two_commit_workspace(scenario: &str, after_stage: &str) -> TempDir {
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&fixture_path(scenario, "before"), root);
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "test"]);
	git(root, &["config", "user.email", "test@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	copy_directory(&fixture_path(scenario, after_stage), root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "change"]);

	tempdir
}

fn run_monochange(root: &Path, args: &[&str]) -> std::process::Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange {args:?}: {error}"))
}

fn parse_json_stdout(output: &std::process::Output) -> Value {
	let stdout = String::from_utf8_lossy(&output.stdout);
	serde_json::from_str(&stdout).unwrap_or_else(|error| {
		panic!(
			"invalid JSON stdout: {error}\nstdout:\n{stdout}\nstderr:\n{}",
			String::from_utf8_lossy(&output.stderr)
		)
	})
}

fn stable_classification_projection(report: &Value) -> Value {
	let packages = report["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("expected packages array in report: {report:#?}"))
		.iter()
		.map(|package| {
			let findings = package["findings"]
				.as_array()
				.unwrap_or_else(|| panic!("expected findings array: {package:#?}"))
				.iter()
				.map(|finding| {
					serde_json::json!({
						"id": finding["id"],
						"rule_id": finding["rule_id"],
						"surface": finding["surface"],
						"change": finding["change"],
						"impact": finding["impact"],
						"bump": finding["bump"],
						"confidence": finding["confidence"],
						"completeness": finding["coverage"]["completeness"],
						"note": finding["coverage"]["note"],
						"summary": finding["summary"],
					})
				})
				.collect::<Vec<_>>();
			serde_json::json!({
				"package_id": package["package_id"],
				"recommendation": package["recommendation"],
				"decision": {
					"compatibility_impact": package["decision"]["compatibility_impact"],
					"proposed_changeset_bump": package["decision"]["proposed_changeset_bump"],
					"enforceable_minimum": package["decision"]["enforceable_minimum"],
					"confidence": package["decision"]["confidence"],
					"completeness": package["decision"]["completeness"],
					"review_required": package["decision"]["review_required"],
				},
				"action": package["action"],
				"findings": findings,
			})
		})
		.collect::<Vec<_>>();

	serde_json::json!({
		"recommendation": report["recommendation"],
		"packages": packages,
	})
}

#[test]
fn inert_lint_attribute_on_constant_does_not_classify_as_breaking() {
	let tempdir = setup_two_commit_workspace("inert-lint-attribute", "after");

	let classify_output = run_monochange(
		tempdir.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);
	assert!(
		classify_output.status.success(),
		"classify failed:\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&classify_output.stdout),
		String::from_utf8_lossy(&classify_output.stderr)
	);
	let classify_json = parse_json_stdout(&classify_output);
	assert_eq!(classify_json["recommendation"], "patch");
	assert_json_snapshot!(stable_classification_projection(&classify_json));

	let affected_output = run_monochange(
		tempdir.path(),
		&[
			"step",
			"affected-packages",
			"--from",
			"HEAD~1",
			"--verify",
			"--format",
			"json",
		],
	);
	assert!(
		affected_output.status.success(),
		"affected-packages --verify should pass with a patch changeset:\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&affected_output.stdout),
		String::from_utf8_lossy(&affected_output.stderr)
	);
	let affected_json = parse_json_stdout(&affected_output);
	assert_eq!(affected_json["status"], "passed");
	assert_eq!(affected_json["errors"], serde_json::json!([]));
}

#[test]
fn trait_default_method_addition_classifies_as_additive_minor() {
	let tempdir = setup_two_commit_workspace("trait-default-method", "after");

	let classify_output = run_monochange(
		tempdir.path(),
		&[
			"change", "classify", "--base", "HEAD~1", "--head", "HEAD", "--format", "json",
		],
	);
	assert!(
		classify_output.status.success(),
		"classify failed:\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&classify_output.stdout),
		String::from_utf8_lossy(&classify_output.stderr)
	);
	let classify_json = parse_json_stdout(&classify_output);
	assert_eq!(classify_json["recommendation"], "minor");
	assert_json_snapshot!(stable_classification_projection(&classify_json));

	let affected_output = run_monochange(
		tempdir.path(),
		&[
			"step",
			"affected-packages",
			"--from",
			"HEAD~1",
			"--verify",
			"--format",
			"json",
		],
	);
	assert!(
		affected_output.status.success(),
		"affected-packages --verify should pass with a minor changeset:\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&affected_output.stdout),
		String::from_utf8_lossy(&affected_output.stderr)
	);
	let affected_json = parse_json_stdout(&affected_output);
	assert_eq!(affected_json["status"], "passed");
	assert_eq!(affected_json["errors"], serde_json::json!([]));
}

#[test]
fn bump_none_changeset_fails_alignment_on_breaking_change() {
	let tempdir = setup_two_commit_workspace("bump-none-alignment", "after-bump-none");

	let affected_output = run_monochange(
		tempdir.path(),
		&[
			"step",
			"affected-packages",
			"--from",
			"HEAD~1",
			"--verify",
			"--format",
			"json",
		],
	);
	assert!(
		!affected_output.status.success(),
		"bump: none changeset should fail alignment on a breaking change"
	);
	let affected_json = parse_json_stdout(&affected_output);
	assert_eq!(affected_json["status"], "failed");
	assert_json_snapshot!(serde_json::json!({
		"status": affected_json["status"],
		"summary": affected_json["summary"],
		"errors": affected_json["errors"],
		"warnings": affected_json["warnings"],
	}));
}

#[test]
fn explicit_version_only_changeset_skips_bump_alignment() {
	let tempdir = setup_two_commit_workspace("bump-none-alignment", "after-explicit-version");

	let affected_output = run_monochange(
		tempdir.path(),
		&[
			"step",
			"affected-packages",
			"--from",
			"HEAD~1",
			"--verify",
			"--format",
			"json",
		],
	);
	assert!(
		affected_output.status.success(),
		"explicit-version-only changeset should skip bump alignment:\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&affected_output.stdout),
		String::from_utf8_lossy(&affected_output.stderr)
	);
	let affected_json = parse_json_stdout(&affected_output);
	assert_eq!(affected_json["status"], "passed");
	assert_eq!(affected_json["errors"], serde_json::json!([]));
}
