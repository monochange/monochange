//! Report rendering for every `monochange ci` subcommand.
//!
//! Each report has a machine-readable JSON form and one or more human forms. The
//! human renderers are assembled section by section so a change to one section
//! cannot silently reorder the others, and `github-outputs` writes the same
//! values to `$GITHUB_OUTPUT` for downstream workflow steps.

use std::collections::BTreeMap;
use std::fs;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use super::types::CiAnalyticsReport;
use super::types::CiAuthReport;
use super::types::CiDecideReport;
use super::types::CiDecisionRecord;
use super::types::CiFalseNegativeIncident;
use super::types::CiFalsePositiveIncident;
use super::types::CiHistoryReport;
use super::types::CiSlowJobReport;
use super::types::CiVerificationRecord;

/// Multiplier that turns a ratio into a percentage.
const PERCENT: f64 = 100.0;

/// Render the auth report as JSON, text, or a bare token.
pub(super) fn render_auth_report(
	report: &CiAuthReport,
	format: &str,
	raw_token_in_text: bool,
) -> MonochangeResult<String> {
	if format == "json" {
		return to_pretty_json(report, "auth");
	}

	if raw_token_in_text {
		return Ok(report.token.clone());
	}

	Ok(format!(
		"action: {}\nauthenticated: {}\nrepo_id: {}\nrepo_hash: {}\ntoken: {}\nauth_file: {}\nsource: {}",
		report.action,
		report.authenticated,
		report.repo_id,
		report.repo_hash,
		report.token,
		report.auth_file,
		report.source
	))
}

/// Render the decision report for the requested output format.
pub(super) fn render_decide_report(
	report: &CiDecideReport,
	format: &str,
) -> MonochangeResult<String> {
	match format {
		"json" => to_pretty_json(report, "decide"),
		"github-outputs" => Ok(render_github_outputs(report)),
		_ => Ok(render_decide_text(report)),
	}
}

/// Render the decision report as GitHub Actions step outputs.
fn render_github_outputs(report: &CiDecideReport) -> String {
	let jobs_map: BTreeMap<&str, &str> = report
		.jobs
		.iter()
		.map(|job| (job.job.as_str(), job.decision.as_str()))
		.collect();
	let jobs_json = serde_json::to_string(&jobs_map).unwrap_or_else(|_| "{}".to_string());
	let lines = [
		format!("decision={}", report.decision),
		format!("run={}", report.run),
		format!("unconditional={}", report.unconditional),
		format!("provider={}", report.provider),
		format!("model={}", report.model),
		format!("reason={}", report.reason.replace('\n', " ")),
		format!("jobs={jobs_json}"),
	];
	let output = lines.join("\n");

	append_github_output(&output);

	output
}

/// Append one output block to `$GITHUB_OUTPUT` when the file is configured.
fn append_github_output(output: &str) {
	let Ok(path) = std::env::var("GITHUB_OUTPUT") else {
		return;
	};

	if path.trim().is_empty() {
		return;
	}

	let _ = fs::OpenOptions::new()
		.create(true)
		.append(true)
		.open(path)
		.and_then(|mut file| {
			use std::io::Write;

			writeln!(file, "{output}")
		});
}

/// Render the decision report as human-readable text.
fn render_decide_text(report: &CiDecideReport) -> String {
	let mut lines = vec![
		format!("decision: {}", report.decision),
		format!("run: {}", report.run),
		format!("unconditional: {}", report.unconditional),
		format!("provider: {} ({})", report.provider, report.model),
		format!("reason: {}", report.reason),
	];

	for job in &report.jobs {
		lines.extend(decision_job_lines(job));
	}

	lines.join("\n")
}

/// Render one job's decision line plus any duration suggestions.
fn decision_job_lines(record: &CiDecisionRecord) -> Vec<String> {
	let mut lines = vec![format!(
		"job `{}`: {} (run={}, unconditional={}, skips_on_main={}) — {}",
		record.job,
		record.decision,
		record.run,
		record.unconditional,
		record.consecutive_skips_on_main,
		record.reason
	)];

	if record.duration_red_flag {
		lines.extend(
			record
				.speed_suggestions
				.iter()
				.map(|suggestion| format!("  speed suggestion: {suggestion}")),
		);
	}

	lines
}

/// Render one verification record.
pub(super) fn render_verification_report(
	verification: &CiVerificationRecord,
	format: &str,
) -> MonochangeResult<String> {
	if format == "json" {
		return to_pretty_json(verification, "verification");
	}

	let mut lines = vec![
		format!("job: {}", verification.job),
		format!("conclusion: {}", verification.conclusion),
		format!("prior_decision: {}", verification.prior_decision),
		format!("classification: {}", verification.classification.as_str()),
		format!("reason: {}", verification.reason),
	];

	if let Some(rule) = verification.learned_rule_added.as_ref() {
		lines.push(format!(
			"learned_rule_added: {} (patterns: {})",
			rule.id,
			rule.patterns.join(", ")
		));
	}

	if verification.duration_red_flag {
		lines.extend(
			verification
				.speed_suggestions
				.iter()
				.map(|suggestion| format!("speed_suggestion: {suggestion}")),
		);
	}

	Ok(lines.join("\n"))
}

/// Render the analytics report for the requested output format.
pub(super) fn render_analytics_report(
	report: &CiAnalyticsReport,
	format: &str,
) -> MonochangeResult<String> {
	match format {
		"json" => to_pretty_json(report, "analytics"),
		"markdown" | "md" => Ok(render_analytics_markdown(report)),
		_ => Ok(render_analytics_text(report)),
	}
}

/// Render the analytics report as Markdown for a PR comment.
fn render_analytics_markdown(report: &CiAnalyticsReport) -> String {
	let mut lines = analytics_markdown_metrics(report);

	lines.extend(analytics_markdown_false_negatives(report));
	lines.extend(analytics_markdown_false_positives(report));
	lines.extend(analytics_markdown_slow_jobs(report));

	lines.join("\n")
}

/// Markdown summary metrics section.
fn analytics_markdown_metrics(report: &CiAnalyticsReport) -> Vec<String> {
	vec![
		format!("# CI Gatekeeper Analytics (`{}`)", report.repo_id),
		String::new(),
		"## Summary Metrics".to_string(),
		format!(
			"- **Total Decisions**: {} (`y`: {}, `n`: {}, unconditional: {})",
			report.total_decisions,
			report.run_decisions,
			report.skip_decisions,
			report.unconditional_decisions
		),
		format!("- **Skip Rate**: {:.1}%", report.skip_rate * PERCENT),
		format!(
			"- **Estimated VM-Minutes Saved**: {:.1}m",
			report.estimated_vm_minutes_saved
		),
		format!(
			"- **Wasted VM-Minutes (False Positives)**: {:.1}m",
			report.wasted_vm_minutes
		),
		format!(
			"- **Classification Counts**: `true_positive`: {}, `true_negative`: {}, `false_positive`: {}, `false_negative`: {}",
			report.true_positives,
			report.true_negatives,
			report.false_positives,
			report.false_negatives
		),
		format!(
			"- **Precision**: {:.1}% | **Recall**: {:.1}% | **False-Negative Rate**: {:.1}%",
			report.precision * PERCENT,
			report.recall * PERCENT,
			report.false_negative_rate * PERCENT
		),
		String::new(),
	]
}

/// Markdown false-negative incident section.
fn analytics_markdown_false_negatives(report: &CiAnalyticsReport) -> Vec<String> {
	let mut lines =
		vec!["## Runs That Didn't Run (`n`) But Should Have (`false_negative`)".to_string()];

	if report.false_negative_incidents.is_empty() {
		lines.push("None.".to_string());
	} else {
		lines.extend(
			report
				.false_negative_incidents
				.iter()
				.map(markdown_false_negative_line),
		);
	}

	lines.push(String::new());

	lines
}

/// Render one false-negative incident line.
fn markdown_false_negative_line(incident: &CiFalseNegativeIncident) -> String {
	let rule_description = incident.learned_rule.as_ref().map_or_else(
		|| "none".to_string(),
		|rule| format!("{} ({})", rule.id, rule.patterns.join(", ")),
	);

	format!(
		"- **Job `{}`** (commit: `{}`, PR: `{}`): files `[{}]` — learned rule: `{}`",
		incident.job,
		incident.commit.as_deref().unwrap_or("n/a"),
		incident.pr.as_deref().unwrap_or("n/a"),
		incident.changed_files.join(", "),
		rule_description
	)
}

/// Markdown false-positive incident section.
fn analytics_markdown_false_positives(report: &CiAnalyticsReport) -> Vec<String> {
	let mut lines =
		vec!["## Runs That Ran (`y`) But Shouldn't Have (`false_positive`)".to_string()];

	if report.false_positive_incidents.is_empty() {
		lines.push("None.".to_string());
	} else {
		lines.extend(
			report
				.false_positive_incidents
				.iter()
				.map(markdown_false_positive_line),
		);
	}

	lines.push(String::new());

	lines
}

/// Render one false-positive incident line.
fn markdown_false_positive_line(incident: &CiFalsePositiveIncident) -> String {
	format!(
		"- **Job `{}`** (commit: `{}`, wasted: {:.1}m): files `[{}]` — {}",
		incident.job,
		incident.commit.as_deref().unwrap_or("n/a"),
		incident.wasted_vm_minutes,
		incident.changed_files.join(", "),
		incident.recommendation
	)
}

/// Markdown slow-job section.
fn analytics_markdown_slow_jobs(report: &CiAnalyticsReport) -> Vec<String> {
	let mut lines = vec!["## Slow Job Red Flags".to_string()];

	if report.slow_jobs.is_empty() {
		lines.push("None.".to_string());
	} else {
		for slow_job in &report.slow_jobs {
			lines.extend(markdown_slow_job_lines(slow_job));
		}
	}

	lines
}

/// Render one slow job with its suggestions.
fn markdown_slow_job_lines(slow_job: &CiSlowJobReport) -> Vec<String> {
	let mut lines = vec![format!(
		"- **Job `{}`**: max {:.1}m (threshold: {}m, rust: {})",
		slow_job.job,
		slow_job.max_duration_seconds as f64 / 60.0,
		slow_job.threshold_minutes,
		slow_job.is_rust_job
	)];

	lines.extend(
		slow_job
			.suggestions
			.iter()
			.map(|suggestion| format!("  - {suggestion}")),
	);

	lines
}

/// Render the analytics report as plain text.
fn render_analytics_text(report: &CiAnalyticsReport) -> String {
	let mut lines = analytics_text_metrics(report);

	lines.extend(analytics_text_false_negatives(report));
	lines.extend(analytics_text_false_positives(report));
	lines.extend(analytics_text_slow_jobs(report));

	lines.join("\n")
}

/// Plain-text summary metrics.
fn analytics_text_metrics(report: &CiAnalyticsReport) -> Vec<String> {
	vec![
		format!("repo_id: {}", report.repo_id),
		format!(
			"total_decisions: {} (y: {}, n: {}, unconditional: {})",
			report.total_decisions,
			report.run_decisions,
			report.skip_decisions,
			report.unconditional_decisions
		),
		format!("skip_rate: {:.1}%", report.skip_rate * PERCENT),
		format!(
			"estimated_vm_minutes_saved: {:.1}",
			report.estimated_vm_minutes_saved
		),
		format!("wasted_vm_minutes: {:.1}", report.wasted_vm_minutes),
		format!(
			"true_positive: {}, true_negative: {}, false_positive: {}, false_negative: {}",
			report.true_positives,
			report.true_negatives,
			report.false_positives,
			report.false_negatives
		),
		format!(
			"precision: {:.3}, recall: {:.3}, false_negative_rate: {:.3}",
			report.precision, report.recall, report.false_negative_rate
		),
	]
}

/// Plain-text false-negative section.
fn analytics_text_false_negatives(report: &CiAnalyticsReport) -> Vec<String> {
	let mut lines = vec!["false_negatives (didn't run `n`, but should have):".to_string()];

	if report.false_negative_incidents.is_empty() {
		lines.push("  none".to_string());
	} else {
		lines.extend(
			report
				.false_negative_incidents
				.iter()
				.map(text_false_negative_line),
		);
	}

	lines
}

/// Render one plain-text false-negative line.
fn text_false_negative_line(incident: &CiFalseNegativeIncident) -> String {
	let rule_description = incident.learned_rule.as_ref().map_or_else(
		|| "none".to_string(),
		|rule| format!("{} [{}]", rule.id, rule.patterns.join(", ")),
	);

	format!(
		"  - job={} commit={} files=[{}] learned_rule={}",
		incident.job,
		incident.commit.as_deref().unwrap_or("n/a"),
		incident.changed_files.join(", "),
		rule_description
	)
}

/// Plain-text false-positive section.
fn analytics_text_false_positives(report: &CiAnalyticsReport) -> Vec<String> {
	let mut lines = vec!["false_positives (ran `y`, but shouldn't have):".to_string()];

	if report.false_positive_incidents.is_empty() {
		lines.push("  none".to_string());
	} else {
		lines.extend(
			report
				.false_positive_incidents
				.iter()
				.map(text_false_positive_line),
		);
	}

	lines
}

/// Render one plain-text false-positive line.
fn text_false_positive_line(incident: &CiFalsePositiveIncident) -> String {
	format!(
		"  - job={} commit={} wasted_minutes={:.1} files=[{}] recommendation={}",
		incident.job,
		incident.commit.as_deref().unwrap_or("n/a"),
		incident.wasted_vm_minutes,
		incident.changed_files.join(", "),
		incident.recommendation
	)
}

/// Plain-text slow-job section.
fn analytics_text_slow_jobs(report: &CiAnalyticsReport) -> Vec<String> {
	let mut lines = vec!["slow_jobs:".to_string()];

	if report.slow_jobs.is_empty() {
		lines.push("  none".to_string());
	} else {
		for slow_job in &report.slow_jobs {
			lines.extend(text_slow_job_lines(slow_job));
		}
	}

	lines
}

/// Render one plain-text slow job with its suggestions.
fn text_slow_job_lines(slow_job: &CiSlowJobReport) -> Vec<String> {
	let mut lines = vec![format!(
		"  - job={} max_seconds={} threshold_minutes={}",
		slow_job.job, slow_job.max_duration_seconds, slow_job.threshold_minutes
	)];

	lines.extend(
		slow_job
			.suggestions
			.iter()
			.map(|suggestion| format!("    suggestion: {suggestion}")),
	);

	lines
}

/// Render the history report for the requested output format.
pub(super) fn render_history_report(
	report: &CiHistoryReport,
	format: &str,
) -> MonochangeResult<String> {
	if format == "json" {
		return to_pretty_json(report, "history");
	}

	let mut lines = vec![
		format!("repo_id: {}", report.repo_id),
		format!("decisions: {}", report.decisions.len()),
		format!("verifications: {}", report.verifications.len()),
		format!("learned_rules: {}", report.learned_rules.len()),
	];

	lines.extend(report.decisions.iter().map(history_decision_line));
	lines.extend(report.verifications.iter().map(history_verification_line));

	Ok(lines.join("\n"))
}

/// Render one decision history line.
fn history_decision_line(record: &CiDecisionRecord) -> String {
	format!(
		"decision [{}]: job={} decision={} unconditional={} provider={} reason={}",
		record.id,
		record.job,
		record.decision,
		record.unconditional,
		record.provider,
		record.reason
	)
}

/// Render one verification history line.
fn history_verification_line(record: &CiVerificationRecord) -> String {
	format!(
		"verification [{}]: job={} conclusion={} classification={} prior={}",
		record.id,
		record.job,
		record.conclusion,
		record.classification.as_str(),
		record.prior_decision
	)
}

/// Serialize a report as pretty JSON with a stable error message.
fn to_pretty_json<T: serde::Serialize>(value: &T, report_name: &str) -> MonochangeResult<String> {
	serde_json::to_string_pretty(value).map_err(|error| {
		MonochangeError::Io(format!("failed to serialize {report_name} report: {error}"))
	})
}
