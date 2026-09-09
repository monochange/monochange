//! Process-level contracts for CLI results and progress output.

use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Output;

use monochange_test_helpers::copy_directory;
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
	assert!(
		stderr.contains("Loading workspace configuration"),
		"{stderr}"
	);
	assert!(
		stderr.contains("Checking workspace configuration"),
		"{stderr}"
	);
	assert!(stderr.contains("Checking cargo version groups"), "{stderr}");
	assert!(!stderr.contains('\r'), "{stderr:?}");
	assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn config_defaults_to_a_concise_human_summary() {
	let root = fixture_path("json-plain-output/release-workspace");
	let output = monochange(&root, &["step", "config"]);
	assert!(
		output.status.success(),
		"config failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);
	let stdout = String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("config stdout must be UTF-8: {error}"));

	assert!(
		stdout.starts_with("Workspace configuration\n\n"),
		"{stdout}"
	);
	assert!(stdout.contains("Packages:"), "{stdout}");
	assert!(stdout.contains("Use `--format json`"), "{stdout}");
	assert!(
		stdout.len() < 1_000,
		"default config output is too large: {} bytes",
		stdout.len()
	);
	assert!(serde_json::from_str::<serde_json::Value>(&stdout).is_err());

	let json = monochange(&root, &["step", "config", "--format", "json"]);
	assert!(json.status.success(), "JSON config failed: {json:#?}");
	serde_json::from_slice::<serde_json::Value>(&json.stdout)
		.unwrap_or_else(|error| panic!("explicit config JSON must parse: {error}"));
}

#[test]
fn quiet_suppresses_output_without_changing_execution() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("cli-output-contract/quiet-command"),
		tempdir.path(),
	);

	let output = monochange(tempdir.path(), &["run", "mutate", "--quiet"]);

	assert!(output.status.success(), "quiet command failed: {output:#?}");
	assert!(output.stdout.is_empty(), "stdout: {:?}", output.stdout);
	assert!(output.stderr.is_empty(), "stderr: {:?}", output.stderr);
	assert_eq!(
		std::fs::read_to_string(tempdir.path().join("quiet-command-ran"))
			.unwrap_or_else(|error| panic!("quiet marker: {error}")),
		"ran",
	);
}

#[test]
fn jq_rejects_implicit_human_output() {
	let root = fixture_path("json-plain-output/release-workspace");
	let output = monochange(&root, &["step", "config", "--jq", ".config.packages"]);
	let stderr = String::from_utf8_lossy(&output.stderr);

	assert!(!output.status.success());
	assert!(
		stdout_is_empty_or_newline(&output.stdout),
		"stdout: {:?}",
		output.stdout
	);
	assert!(
		stderr.contains("--jq requires explicit JSON output"),
		"{stderr}"
	);
	assert!(stderr.contains("error[cli.json_required]"), "{stderr}");
	assert!(
		stderr.contains("command: monochange step config"),
		"{stderr}"
	);
	assert!(stderr.contains("help: Add `--format json`"), "{stderr}");
}

#[test]
fn slow_captured_command_names_its_active_phase() {
	let root = fixture_path("monochange/release-progress");
	let output = monochange(&root, &["run", "progress-spinner"]);
	let stderr = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("progress stderr must be UTF-8: {error}"));

	assert!(
		output.status.success(),
		"slow command failed\nstdout:\n{}\nstderr:\n{stderr}",
		String::from_utf8_lossy(&output.stdout),
	);
	assert!(
		stderr.contains("Loading workspace configuration"),
		"{stderr}"
	);
	assert!(stderr.contains("[1/1] slow spinner"), "{stderr}");
	assert!(
		stderr.contains("running command `sleep 1.5; echo done`"),
		"{stderr}"
	);
	assert!(!stderr.contains('\r'), "{stderr:?}");
	assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
}

#[test]
fn check_failure_separates_the_result_from_the_actionable_diagnostic() {
	let root = fixture_path("check-output/npm-workspace");
	let output = monochange(&root, &["check", "--format", "text"]);
	let stdout = String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("check stdout must be UTF-8: {error}"));
	let stderr = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("check stderr must be UTF-8: {error}"));

	assert!(!output.status.success());
	assert!(stdout.contains("lint: 6 errors, 1 warnings"), "{stdout}");
	assert!(stdout.contains("npm/workspace-protocol"), "{stdout}");
	assert!(
		stderr.contains("error[check.failed]: check failed: 6 errors, 1 warning"),
		"{stderr}"
	);
	assert!(stderr.contains("command: monochange check"), "{stderr}");
	assert!(stderr.contains("help:"), "{stderr}");
	assert!(!stderr.contains("npm/workspace-protocol"), "{stderr}");
}

#[test]
fn json_progress_uses_the_same_bootstrap_and_workflow_events() {
	let root = fixture_path("monochange/release-progress");
	let output = monochange(
		&root,
		&["run", "progress-spinner", "--progress-format", "json"],
	);
	assert!(output.status.success(), "JSON progress failed: {output:#?}");
	let events = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("progress stderr must be UTF-8: {error}"))
		.lines()
		.map(|line| {
			serde_json::from_str::<serde_json::Value>(line)
				.unwrap_or_else(|error| panic!("parse progress event `{line}`: {error}"))
		})
		.collect::<Vec<_>>();
	let names = events
		.iter()
		.filter_map(|event| event.get("event").and_then(serde_json::Value::as_str))
		.collect::<Vec<_>>();
	assert!(names.contains(&"phase_started"), "{events:#?}");
	assert!(names.contains(&"command_started"), "{events:#?}");
	assert!(names.contains(&"step_started"), "{events:#?}");
	assert!(names.contains(&"command_output"), "{events:#?}");
	assert!(names.contains(&"command_finished"), "{events:#?}");
	for (expected, event) in events.iter().enumerate() {
		assert_eq!(event.get("sequence"), Some(&serde_json::json!(expected)));
	}
}

fn stdout_is_empty_or_newline(stdout: &[u8]) -> bool {
	stdout.is_empty() || stdout == b"\n"
}
