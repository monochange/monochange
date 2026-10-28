//! Environment checks for monochange workspaces.
//!
//! `monochange doctor` reports whether the tools a workspace actually uses
//! are installed and runnable, so a broken environment surfaces in one place
//! instead of as a failure deep inside a release step. Every check is local:
//! the command never touches the network.

#[cfg(test)]
#[path = "__tests__/doctor_tests.rs"]
mod tests;

use std::path::Path;
use std::process::Command;

use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use serde::Serialize;

use crate::OutputFormat;
use crate::output::text::Outcome;
use crate::output::text::TextReport;
use crate::output::text::TextTheme;

/// One environment check result.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DoctorCheck {
	/// What was checked, e.g. `git` or `ecosystem cargo (cargo)`.
	pub label: String,
	/// `ok` with a version string, or `missing` with install guidance.
	pub status: DoctorStatus,
}

/// Outcome of one check.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DoctorStatus {
	/// The tool ran and reported its version.
	Ok(String),
	/// The tool is not installed or not runnable.
	Missing(String),
}

/// Everything `monochange doctor` reports for a workspace.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct DoctorReport {
	/// Path the report was produced for.
	pub root: String,
	/// All checks, in report order.
	pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
	/// The first line a reader needs: how many tools are missing.
	#[must_use]
	pub(crate) fn summary_text(&self) -> String {
		let missing = self
			.checks
			.iter()
			.filter(|check| matches!(check.status, DoctorStatus::Missing(_)))
			.count();

		match missing {
			0 => format!("all {} checks passed", self.checks.len()),
			1 => "1 check needs attention".to_string(),
			count => format!("{count} checks need attention"),
		}
	}
}

/// The tool each ecosystem's release steps shell out to.
const ECOSYSTEM_TOOLS: &[(Ecosystem, &str)] = &[
	(Ecosystem::Cargo, "cargo"),
	(Ecosystem::Npm, "node"),
	(Ecosystem::Deno, "deno"),
	(Ecosystem::Dart, "dart"),
	(Ecosystem::Python, "python3"),
	(Ecosystem::Go, "go"),
	(Ecosystem::Swift, "swift"),
	(Ecosystem::CocoaPods, "pod"),
	(Ecosystem::Gradle, "gradle"),
];

/// Whether the ecosystem is enabled for this workspace. Settings without an
/// explicit flag default to enabled, matching discovery behavior.
///
/// `None` covers an ecosystem this monochange build has no settings field
/// for, which doctor then skips: it has no tool mapped either.
fn ecosystem_settings(
	configuration: &WorkspaceConfiguration,
	ecosystem: Ecosystem,
) -> Option<&monochange_core::EcosystemSettings> {
	let settings = match ecosystem {
		Ecosystem::Cargo => &configuration.cargo,
		Ecosystem::Npm => &configuration.npm,
		Ecosystem::Deno => &configuration.deno,
		Ecosystem::Dart => &configuration.dart,
		Ecosystem::Python => &configuration.python,
		Ecosystem::Go => &configuration.go,
		Ecosystem::Swift => &configuration.swift,
		Ecosystem::CocoaPods => &configuration.cocoapods,
		Ecosystem::Gradle => &configuration.gradle,
		_ => return None,
	};

	Some(settings)
}

/// Run `<tool> --version` and capture its first output line.
fn tool_version(tool: &str) -> DoctorStatus {
	let output = Command::new(tool).arg("--version").output();

	let output = match output {
		Ok(output) if output.status.success() => output,
		_ => {
			return DoctorStatus::Missing(format!("install `{tool}` and make sure it is on PATH"));
		}
	};

	let version = String::from_utf8_lossy(&output.stdout)
		.lines()
		.chain(String::from_utf8_lossy(&output.stderr).lines())
		.find(|line| !line.trim().is_empty())
		.unwrap_or_default()
		.trim()
		.to_string();

	if version.is_empty() {
		return DoctorStatus::Missing(format!("`{tool}` ran but printed no version"));
	}

	DoctorStatus::Ok(version)
}

/// Collect every check for the workspace at `root`.
pub(crate) fn collect_doctor_report(
	root: &Path,
	configuration: &MonochangeResult<WorkspaceConfiguration>,
) -> DoctorReport {
	let mut checks = Vec::new();

	checks.push(DoctorCheck {
		label: "git".to_string(),
		status: tool_version("git"),
	});

	checks.push(match configuration {
		Ok(configuration) => {
			DoctorCheck {
				label: "monochange.toml".to_string(),
				status: DoctorStatus::Ok(format!(
					"parsed with {} packages and {} groups",
					configuration.packages.len(),
					configuration.groups.len(),
				)),
			}
		}
		Err(error) => {
			DoctorCheck {
				label: "monochange.toml".to_string(),
				status: DoctorStatus::Missing(error.to_string()),
			}
		}
	});

	if let Ok(configuration) = configuration {
		for (ecosystem, tool) in ECOSYSTEM_TOOLS {
			let Some(settings) = ecosystem_settings(configuration, *ecosystem) else {
				continue;
			};

			if !settings.enabled.unwrap_or(true) {
				continue;
			}

			checks.push(DoctorCheck {
				label: format!("{} ecosystem ({tool})", ecosystem.as_str()),
				status: tool_version(tool),
			});
		}
	}

	DoctorReport {
		root: root.display().to_string(),
		checks,
	}
}

/// Render the doctor report as text or JSON.
pub(crate) fn render_doctor_report(
	root: &Path,
	configuration: &MonochangeResult<WorkspaceConfiguration>,
	format: OutputFormat,
) -> MonochangeResult<String> {
	let report = collect_doctor_report(root, configuration);

	if matches!(format, OutputFormat::Json | OutputFormat::JsonMin) {
		return format.render_json_value(&report, "doctor report");
	}

	let mut text = TextReport::new(TextTheme::for_stdout());
	text.section("Environment checks", None);

	let lines = report
		.checks
		.iter()
		.map(|check| {
			match &check.status {
				DoctorStatus::Ok(version) => format!("{} — {version}", check.label),
				DoctorStatus::Missing(guidance) => format!("{} — MISSING: {guidance}", check.label),
			}
		})
		.collect::<Vec<_>>();
	text.list(lines, 32);

	let missing: Vec<String> = report
		.checks
		.iter()
		.filter(|check| matches!(check.status, DoctorStatus::Missing(_)))
		.map(|check| check.label.clone())
		.collect();

	let outcome = if missing.is_empty() {
		Outcome::Success
	} else {
		Outcome::Warning
	};
	text.summary(outcome, &report.summary_text(), &[]);

	let rendered = text.render();
	if rendered.trim().is_empty() {
		return Err(MonochangeError::Discovery(
			"doctor produced an empty report".to_string(),
		));
	}

	Ok(rendered)
}
