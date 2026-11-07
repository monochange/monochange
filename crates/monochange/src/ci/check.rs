//! The `ci check` subcommand.
//!
//! `decide` is only as safe as the `[ci]` configuration behind it: a root
//! package that no longer exists, a glob that matches nothing, or a workflow
//! that references a job id nobody configured all silently narrow CI coverage.
//! `ci check` fails with the full list of those problems instead.

use std::path::Path;

use clap::ArgMatches;
use monochange_config::CiSettings;
use monochange_config::load_ci_settings;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use regex::Regex;

use super::args;
use super::graph::WorkspaceGraph;
use super::graph::build_workspace_graph;
use super::graph::glob_match;

/// One configuration problem reported by `ci check`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CiCheckProblem {
	kind: &'static str,
	message: String,
}

/// Validate `[ci]` configuration and optional workflow wiring.
pub(super) fn run_ci_check_subcommand(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	let settings = load_ci_settings(root)?;
	let graph = build_workspace_graph(root);
	let problems = collect_problems(root, &settings, &graph, matches);

	if problems.is_empty() {
		return render_success(&settings, &graph, matches);
	}

	let rendered = render_problems(&problems, matches);

	Err(MonochangeError::Diagnostic(rendered))
}

/// Collect every configuration problem in one pass.
fn collect_problems(
	root: &Path,
	settings: &CiSettings,
	graph: &WorkspaceGraph,
	matches: &ArgMatches,
) -> Vec<CiCheckProblem> {
	let mut problems = Vec::new();
	let identifiers = graph.package_identifiers();
	let tracked_files = tracked_files(root);

	for (job_id, job) in &settings.jobs {
		if !is_kebab_case(job_id) {
			problems.push(CiCheckProblem {
				kind: "job-id",
				message: format!("job id `{job_id}` is not kebab-case"),
			});
		}

		if job.packages.is_empty() && job.paths.is_empty() && !job.always {
			problems.push(CiCheckProblem {
				kind: "empty-job",
				message: format!(
					"job `{job_id}` sets none of `packages`, `paths`, or `always`; it would never run"
				),
			});
		}

		problems.extend(package_problems(job_id, &job.packages, &identifiers));
		problems.extend(path_problems(job_id, &job.paths, &tracked_files));
	}

	problems.extend(workflow_problems(root, settings, matches));

	problems
}

/// Validate one job's root package entries.
fn package_problems(
	job_id: &str,
	packages: &[String],
	identifiers: &[String],
) -> Vec<CiCheckProblem> {
	packages
		.iter()
		.filter_map(|entry| {
			let matched = identifiers
				.iter()
				.any(|identifier| glob_match(identifier, entry));

			if matched {
				return None;
			}

			Some(CiCheckProblem {
				kind: "package",
				message: if entry.contains('*') || entry.contains('?') {
					format!("job `{job_id}` package glob `{entry}` matches zero workspace packages")
				} else {
					format!("job `{job_id}` names unknown root package `{entry}`")
				},
			})
		})
		.collect()
}

/// Validate one job's path entries against the tracked file list.
fn path_problems(job_id: &str, paths: &[String], tracked_files: &[String]) -> Vec<CiCheckProblem> {
	paths
		.iter()
		.filter_map(|entry| {
			let matched = tracked_files.iter().any(|file| glob_match(file, entry));

			if matched {
				return None;
			}

			Some(CiCheckProblem {
				kind: "path",
				message: format!("job `{job_id}` path glob `{entry}` matches zero tracked files"),
			})
		})
		.collect()
}

/// Compare workflow references against configured job ids.
fn workflow_problems(
	root: &Path,
	settings: &CiSettings,
	matches: &ArgMatches,
) -> Vec<CiCheckProblem> {
	let Some(workflows) = matches.get_many::<String>("workflow") else {
		return Vec::new();
	};

	let mut problems = Vec::new();
	let mut referenced: Vec<String> = Vec::new();

	for relative in workflows {
		let path = root.join(relative.as_str());

		match std::fs::read_to_string(&path) {
			Ok(contents) => referenced.extend(referenced_job_ids(&contents)),

			Err(error) => {
				problems.push(CiCheckProblem {
					kind: "workflow",
					message: format!("could not read workflow `{}`: {error}", path.display()),
				});
			}
		}
	}

	referenced.sort();
	referenced.dedup();

	for job_id in &referenced {
		if !settings.jobs.contains_key(job_id) {
			problems.push(CiCheckProblem {
				kind: "workflow",
				message: format!(
					"workflow references job `{job_id}` through `decisions` but `[ci.jobs.{job_id}]` is not configured"
				),
			});
		}
	}

	for job_id in settings.jobs.keys() {
		if !referenced.contains(job_id) {
			problems.push(CiCheckProblem {
				kind: "workflow",
				message: format!(
					"configured job `{job_id}` is never referenced by the scanned workflows"
				),
			});
		}
	}

	problems
}

/// Extract job ids referenced through the `decisions` output.
///
/// Matches `fromJSON(needs.<job>.outputs.decisions)['<id>']` and the dot
/// accessor form; a plain regex scan is enough because workflow expressions
/// are line-oriented.
fn referenced_job_ids(workflow_text: &str) -> Vec<String> {
	let bracket =
		Regex::new(r#"fromJSON\(needs\.[A-Za-z0-9_-]+\.outputs\.decisions\)\[['"]([^'"]+)['"]\]"#)
			.unwrap_or_else(|error| panic!("decisions bracket regex must compile: {error}"));
	let dot =
		Regex::new(r"fromJSON\(needs\.[A-Za-z0-9_-]+\.outputs\.decisions\)\.([A-Za-z0-9_-]+)")
			.unwrap_or_else(|error| panic!("decisions dot regex must compile: {error}"));
	let mut ids: Vec<String> = bracket
		.captures_iter(workflow_text)
		.filter_map(|captures| captures.get(1).map(|id| id.as_str().to_string()))
		.collect();

	ids.extend(
		dot.captures_iter(workflow_text)
			.filter_map(|captures| captures.get(1).map(|id| id.as_str().to_string())),
	);

	ids
}

/// List files tracked by git, relative to the repository root.
fn tracked_files(root: &Path) -> Vec<String> {
	std::process::Command::new("git")
		.current_dir(root)
		.args(["ls-files"])
		.output()
		.ok()
		.filter(|output| output.status.success())
		.map(|output| {
			String::from_utf8_lossy(&output.stdout)
				.lines()
				.map(ToString::to_string)
				.collect()
		})
		.unwrap_or_default()
}

/// Test whether a job id is lowercase kebab-case.
fn is_kebab_case(job_id: &str) -> bool {
	!job_id.is_empty()
		&& job_id.chars().all(|character| {
			character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
		}) && !job_id.starts_with('-')
		&& !job_id.ends_with('-')
		&& !job_id.contains("--")
}

/// Render the success message for the requested format.
fn render_success(
	settings: &CiSettings,
	graph: &WorkspaceGraph,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	if args::format_arg(matches) == "json" {
		return serde_json::to_string_pretty(&serde_json::json!({
			"ok": true,
			"jobs": settings.jobs.keys().collect::<Vec<_>>(),
		}))
		.map_err(|error| {
			MonochangeError::Io(format!("failed to serialize check report: {error}"))
		});
	}

	Ok(format!(
		"ci configuration valid: {} job(s) checked (graph: {})",
		settings.jobs.len(),
		graph.source_label()
	))
}

/// Render every problem for the requested format.
fn render_problems(problems: &[CiCheckProblem], matches: &ArgMatches) -> String {
	if args::format_arg(matches) == "json" {
		let payload = serde_json::json!({
			"ok": false,
			"problems": problems
				.iter()
				.map(|problem| serde_json::json!({
					"kind": problem.kind,
					"message": problem.message,
				}))
				.collect::<Vec<_>>(),
		});

		return serde_json::to_string(&payload).unwrap_or_else(|_| "{\"ok\":false}".to_string());
	}

	let mut lines = vec![format!("ci check found {} problem(s):", problems.len())];

	lines.extend(
		problems
			.iter()
			.map(|problem| format!("  [{}] {}", problem.kind, problem.message)),
	);

	lines.join("\n")
}
