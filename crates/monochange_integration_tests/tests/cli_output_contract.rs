//! Process-level contracts for CLI results and progress output.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use monochange_test_helpers::get_cargo_bin;

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../fixtures/tests/{relative}"))
}

fn monochange(root: &Path, args: &[&str]) -> Output {
	Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("NO_COLOR", "1")
		.env("TERM", "dumb")
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run monochange: {error}"))
}

#[test]
fn check_lint_errors_fail_in_every_output_format() {
	let root = fixture_path("check-output/npm-workspace");

	for format in ["text", "markdown", "json", "json-min"] {
		let output = monochange(&root, &["check", "--format", format]);

		assert!(
			!output.status.success(),
			"{format} returned success despite lint errors\nstdout:\n{}\nstderr:\n{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr),
		);

		if matches!(format, "json" | "json-min") {
			let report: serde_json::Value = serde_json::from_slice(&output.stdout)
				.unwrap_or_else(|error| panic!("parse {format} report: {error}"));
			assert_eq!(report["error_count"], serde_json::json!(6));
			assert_eq!(report["warning_count"], serde_json::json!(1));
		}
	}
}

#[test]
fn disabled_check_progress_emits_only_the_failure_diagnostic() {
	let root = fixture_path("check-output/npm-workspace");
	let output = Command::new(get_cargo_bin("monochange"))
		.current_dir(root)
		.env("MONOCHANGE_NO_PROGRESS", "1")
		.env("NO_COLOR", "1")
		.env("TERM", "dumb")
		.args(["check", "--format", "text"])
		.output()
		.unwrap_or_else(|error| panic!("run monochange check: {error}"));
	let stderr = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("check stderr must be UTF-8: {error}"));

	assert!(!output.status.success());
	assert!(!stderr.contains("Validating workspace"), "{stderr}");
	assert!(!stderr.contains("Running 4 suites"), "{stderr}");
	assert!(!stderr.contains('\r'), "{stderr:?}");
	assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn captured_workflow_progress_has_no_terminal_control_sequences() {
	let root = fixture_path("json-plain-output/release-workspace");
	let output = monochange(&root, &["step", "validate"]);
	let stderr = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("validate stderr must be UTF-8: {error}"));

	assert!(
		output.status.success(),
		"validate failed\nstdout:\n{}\nstderr:\n{stderr}",
		String::from_utf8_lossy(&output.stdout),
	);
	assert!(
		stderr.contains("monochange running `step validate`"),
		"{stderr}"
	);
	assert!(!stderr.contains('\r'), "{stderr:?}");
	assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}
