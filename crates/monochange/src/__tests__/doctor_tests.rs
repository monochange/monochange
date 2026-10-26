//! Tests for the `monochange doctor` environment checks.

use std::fs;

use monochange_config::load_workspace_configuration;
use tempfile::tempdir;

use super::DoctorCheck;
use super::DoctorStatus;
use super::collect_doctor_report;
use crate::OutputFormat;
use crate::doctor::render_doctor_report;

fn write_config(contents: &str) -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("monochange.toml"), contents)
		.unwrap_or_else(|error| panic!("write config: {error}"));

	tempdir
}

#[test]
fn doctor_reports_git_and_config_checks() {
	let workspace = write_config("");
	let configuration = load_workspace_configuration(workspace.path());

	let report = collect_doctor_report(workspace.path(), &configuration);

	let git_check = report
		.checks
		.iter()
		.find(|check| check.label == "git")
		.expect("git is always probed");
	assert!(
		matches!(&git_check.status, DoctorStatus::Ok(version) if version.contains("git version")),
		"git check: {git_check:?}",
	);

	let config_check = report
		.checks
		.iter()
		.find(|check| check.label == "monochange.toml")
		.expect("the config is always checked");
	assert!(
		matches!(&config_check.status, DoctorStatus::Ok(summary) if summary.contains("0 packages")),
		"config check: {config_check:?}",
	);
}

#[test]
fn doctor_only_checks_enabled_ecosystem_toolchains() {
	let workspace = write_config("[ecosystems.deno]\nenabled = false\n");
	let configuration = load_workspace_configuration(workspace.path())
		.unwrap_or_else(|error| panic!("fixture config: {error}"));

	let report = collect_doctor_report(workspace.path(), &Ok(configuration));

	let checked = |ecosystem: &str| {
		report
			.checks
			.iter()
			.any(|check| check.label.starts_with(&format!("{ecosystem} ecosystem")))
	};

	// Ecosystems without an explicit flag default to enabled; only deno is off.
	assert!(checked("cargo"), "cargo defaults to enabled");
	assert!(checked("npm"), "npm defaults to enabled");
	assert!(!checked("deno"), "deno is disabled and must be skipped");
}

#[test]
fn missing_tools_count_in_the_summary() {
	let workspace = write_config("");
	let configuration = load_workspace_configuration(workspace.path());

	let mut report = collect_doctor_report(workspace.path(), &configuration);
	let already_missing = report
		.checks
		.iter()
		.filter(|check| matches!(check.status, DoctorStatus::Missing(_)))
		.count();
	// Force one more failing check so the summary counts it, independent of
	// which tools this machine happens to have installed.
	report.checks.push(DoctorCheck {
		label: "definitely-not-a-real-tool".to_string(),
		status: DoctorStatus::Missing("install it".to_string()),
	});

	let expected = match already_missing + 1 {
		1 => "1 check needs attention".to_string(),
		count => format!("{count} checks need attention"),
	};
	assert_eq!(report.summary_text(), expected);
}

#[test]
fn config_failures_surface_as_a_missing_check() {
	let workspace = write_config("this is not = valid = toml\n");
	let configuration = load_workspace_configuration(workspace.path());

	let report = collect_doctor_report(workspace.path(), &configuration);
	let config_check = report
		.checks
		.iter()
		.find(|check| check.label == "monochange.toml")
		.expect("config check always runs");

	assert!(
		matches!(&config_check.status, DoctorStatus::Missing(reason) if !reason.is_empty()),
		"broken config must be reported: {config_check:?}",
	);
	// A broken config cannot say which ecosystems are enabled.
	assert!(
		!report
			.checks
			.iter()
			.any(|check| check.label.contains("ecosystem")),
		"ecosystem checks need a parsed config: {:?}",
		report.checks,
	);
}

#[test]
fn doctor_renders_text_and_json() {
	let workspace = write_config("[ecosystems.deno]\nenabled = false\n");
	let configuration = load_workspace_configuration(workspace.path());

	let text = render_doctor_report(workspace.path(), &configuration, OutputFormat::Text)
		.unwrap_or_else(|error| panic!("text report: {error}"));
	assert!(text.contains("Environment checks"), "text report: {text}");

	let json = render_doctor_report(workspace.path(), &configuration, OutputFormat::Json)
		.unwrap_or_else(|error| panic!("json report: {error}"));
	assert!(json.contains("\"checks\""), "json report: {json}");
}
