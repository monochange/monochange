//! The `ci analytics` subcommand.
//!
//! Analytics is always authenticated: the report names real jobs, commits, and
//! wasted minutes, so it is only ever computed for the repository that owns the
//! memory store. Every metric here is derived from recorded decisions and
//! verifications rather than sampled at read time.

use std::collections::BTreeMap;
use std::path::Path;

use clap::ArgMatches;
use monochange_core::MonochangeResult;

use super::DEFAULT_MAX_DURATION_MINUTES;
use super::DEFAULT_RUST_MAX_DURATION_MINUTES;
use super::args;
use super::auth;
use super::graph::build_speed_suggestions;
use super::graph::duration_threshold_minutes;
use super::graph::exceeds_duration_threshold;
use super::graph::is_rust_job_or_context;
use super::render::render_analytics_report;
use super::types::CiAnalyticsReport;
use super::types::CiAuthReport;
use super::types::CiFalseNegativeIncident;
use super::types::CiFalsePositiveIncident;
use super::types::CiMemoryStore;
use super::types::CiSlowJobReport;
use super::types::CiVerificationClassification;
use super::types::CiVerificationRecord;

/// Assumed job duration when no samples exist, in minutes.
const FALLBACK_JOB_MINUTES: f64 = 5.0;
/// Seconds in a minute.
const SECONDS_PER_MINUTE: f64 = 60.0;
/// Neutral precision/recall value when there is nothing to divide by.
const PERFECT_RATIO: f64 = 1.0;
/// Neutral false-negative rate when there is nothing to divide by.
const ZERO_RATIO: f64 = 0.0;

/// Duration thresholds that depend on the job type.
#[derive(Debug, Clone, Copy)]
struct AnalyticsThresholds {
	max_duration_minutes: u64,
	rust_max_duration_minutes: u64,
}

/// Headline decision counters.
struct DecisionCounts {
	total: usize,
	run: usize,
	skip: usize,
	unconditional: usize,
	skip_rate: f64,
	estimated_vm_minutes_saved: f64,
}

/// Per-classification counters and incident lists.
struct VerificationTallies {
	true_positives: usize,
	true_negatives: usize,
	false_positives: usize,
	false_negatives: usize,
	false_negative_incidents: Vec<CiFalseNegativeIncident>,
	false_positive_incidents: Vec<CiFalsePositiveIncident>,
	wasted_vm_minutes: f64,
}

/// Compute and render the protected analytics report.
pub(super) fn run_ci_analytics_subcommand(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	let repo_id_override = args::repo_id_override(matches);
	let explicit_token = args::explicit_token(matches);

	// Analytics always proves repository ownership before reading the store.
	let auth = auth::authenticate_repo_access(root, repo_id_override, explicit_token)?;
	let memory = auth::load_ci_memory_store(root, repo_id_override, true, explicit_token)?;
	let thresholds = AnalyticsThresholds {
		max_duration_minutes: args::duration_minutes_arg(
			matches,
			"max-duration-minutes",
			DEFAULT_MAX_DURATION_MINUTES,
		),
		rust_max_duration_minutes: args::duration_minutes_arg(
			matches,
			"rust-max-duration-minutes",
			DEFAULT_RUST_MAX_DURATION_MINUTES,
		),
	};
	let report = compute_analytics_report(root, &auth, &memory, thresholds);

	render_analytics_report(&report, args::format_arg(matches))
}

/// Build the analytics report from the memory store.
fn compute_analytics_report(
	root: &Path,
	auth: &CiAuthReport,
	memory: &CiMemoryStore,
	thresholds: AnalyticsThresholds,
) -> CiAnalyticsReport {
	let job_durations = collect_job_durations(memory);
	let counts = decision_counts(memory, &job_durations);
	let tallies = verification_tallies(memory, &job_durations);
	let slow_jobs = slow_job_reports(root, &job_durations, thresholds);
	let tuning_recommendations = tuning_recommendations(&tallies, memory, &slow_jobs);
	let true_positive_total = tallies.true_positives;
	let positive_total = tallies.true_positives + tallies.false_positives;
	let predicted_positive_total = tallies.true_positives + tallies.false_negatives;

	CiAnalyticsReport {
		repo_id: auth.repo_id.clone(),
		repo_hash: auth.repo_hash.clone(),
		authenticated: true,
		total_decisions: counts.total,
		run_decisions: counts.run,
		skip_decisions: counts.skip,
		unconditional_decisions: counts.unconditional,
		skip_rate: counts.skip_rate,
		estimated_vm_minutes_saved: counts.estimated_vm_minutes_saved,
		wasted_vm_minutes: tallies.wasted_vm_minutes,
		true_positives: tallies.true_positives,
		true_negatives: tallies.true_negatives,
		false_positives: tallies.false_positives,
		false_negatives: tallies.false_negatives,
		precision: ratio(true_positive_total, positive_total, PERFECT_RATIO),
		recall: ratio(true_positive_total, predicted_positive_total, PERFECT_RATIO),
		false_negative_rate: ratio(
			tallies.false_negatives,
			predicted_positive_total,
			ZERO_RATIO,
		),
		learned_rules_count: memory.learned_rules.len(),
		false_negative_incidents: tallies.false_negative_incidents,
		false_positive_incidents: tallies.false_positive_incidents,
		tuning_recommendations,
		slow_jobs,
	}
}

/// Group observed durations by job, ignoring zero-second samples.
fn collect_job_durations(memory: &CiMemoryStore) -> BTreeMap<String, Vec<u64>> {
	let mut job_durations: BTreeMap<String, Vec<u64>> = BTreeMap::new();

	for verification in &memory.verifications {
		if verification.duration_seconds > 0 {
			job_durations
				.entry(verification.job.clone())
				.or_default()
				.push(verification.duration_seconds);
		}
	}

	for decision in &memory.decisions {
		if let Some(seconds) = decision.observed_duration_seconds
			&& seconds > 0
		{
			job_durations
				.entry(decision.job.clone())
				.or_default()
				.push(seconds);
		}
	}

	job_durations
}

/// Average duration for a job, in minutes.
fn average_job_minutes(job_durations: &BTreeMap<String, Vec<u64>>, job: &str) -> f64 {
	let Some(samples) = job_durations.get(job).filter(|samples| !samples.is_empty()) else {
		return FALLBACK_JOB_MINUTES;
	};

	let sum: u64 = samples.iter().sum();

	(sum as f64 / samples.len() as f64) / SECONDS_PER_MINUTE
}

/// Count decisions and estimate saved minutes from skipped jobs.
fn decision_counts(
	memory: &CiMemoryStore,
	job_durations: &BTreeMap<String, Vec<u64>>,
) -> DecisionCounts {
	let total = memory.decisions.len();
	let run = memory
		.decisions
		.iter()
		.filter(|decision| decision.run)
		.count();
	let skip = memory
		.decisions
		.iter()
		.filter(|decision| !decision.run)
		.count();
	let unconditional = memory
		.decisions
		.iter()
		.filter(|decision| decision.unconditional)
		.count();
	let estimated_vm_minutes_saved = memory
		.decisions
		.iter()
		.filter(|decision| !decision.run)
		.map(|decision| average_job_minutes(job_durations, &decision.job))
		.sum();

	DecisionCounts {
		total,
		run,
		skip,
		unconditional,
		skip_rate: ratio(skip, total, ZERO_RATIO),
		estimated_vm_minutes_saved,
	}
}

/// Count verification classifications and build incident records.
fn verification_tallies(
	memory: &CiMemoryStore,
	job_durations: &BTreeMap<String, Vec<u64>>,
) -> VerificationTallies {
	let mut tallies = VerificationTallies {
		true_positives: 0,
		true_negatives: 0,
		false_positives: 0,
		false_negatives: 0,
		false_negative_incidents: Vec::new(),
		false_positive_incidents: Vec::new(),
		wasted_vm_minutes: 0.0,
	};

	for verification in &memory.verifications {
		accumulate_verification(&mut tallies, verification, job_durations);
	}

	tallies
}

/// Fold one verification into the tallies.
fn accumulate_verification(
	tallies: &mut VerificationTallies,
	verification: &CiVerificationRecord,
	job_durations: &BTreeMap<String, Vec<u64>>,
) {
	match verification.classification {
		CiVerificationClassification::TruePositive => tallies.true_positives += 1,
		CiVerificationClassification::TrueNegative => tallies.true_negatives += 1,
		CiVerificationClassification::FalseNegative => {
			tallies.false_negatives += 1;
			tallies
				.false_negative_incidents
				.push(false_negative_incident(verification));
		}
		CiVerificationClassification::FalsePositive => {
			tallies.false_positives += 1;

			let wasted_minutes = wasted_minutes(verification, job_durations);

			tallies.wasted_vm_minutes += wasted_minutes;
			tallies
				.false_positive_incidents
				.push(false_positive_incident(verification, wasted_minutes));
		}
	}
}

/// Build a false-negative incident from a verification record.
fn false_negative_incident(verification: &CiVerificationRecord) -> CiFalseNegativeIncident {
	CiFalseNegativeIncident {
		id: verification.id.clone(),
		job: verification.job.clone(),
		commit: verification.commit.clone(),
		pr: verification.pr.clone(),
		changed_files: verification.changed_files.clone(),
		reason: verification.reason.clone(),
		learned_rule: verification.learned_rule_added.clone(),
		timestamp: verification.timestamp.clone(),
	}
}

/// Build a false-positive incident from a verification record.
fn false_positive_incident(
	verification: &CiVerificationRecord,
	wasted_minutes: f64,
) -> CiFalsePositiveIncident {
	CiFalsePositiveIncident {
		id: verification.id.clone(),
		job: verification.job.clone(),
		commit: verification.commit.clone(),
		pr: verification.pr.clone(),
		changed_files: verification.changed_files.clone(),
		wasted_vm_minutes: wasted_minutes,
		reason: verification.reason.clone(),
		recommendation: format!(
			"Restrict job `{}` with `--job-packages` or `--job-paths`, or add ignored paths for `{}` so unrelated changes skip (`n`).",
			verification.job,
			verification.changed_files.join(", ")
		),
		timestamp: verification.timestamp.clone(),
	}
}

/// Minutes wasted by a run that should have been skipped.
fn wasted_minutes(
	verification: &CiVerificationRecord,
	job_durations: &BTreeMap<String, Vec<u64>>,
) -> f64 {
	if verification.duration_seconds > 0 {
		return verification.duration_seconds as f64 / SECONDS_PER_MINUTE;
	}

	average_job_minutes(job_durations, &verification.job)
}

/// Report every job whose longest sample exceeds its threshold.
fn slow_job_reports(
	root: &Path,
	job_durations: &BTreeMap<String, Vec<u64>>,
	thresholds: AnalyticsThresholds,
) -> Vec<CiSlowJobReport> {
	job_durations
		.iter()
		.filter_map(|(job, samples)| slow_job_report(root, job, samples, thresholds))
		.collect()
}

/// Build a slow-job report when the maximum duration is over the threshold.
fn slow_job_report(
	root: &Path,
	job: &str,
	samples: &[u64],
	thresholds: AnalyticsThresholds,
) -> Option<CiSlowJobReport> {
	let max_duration_seconds = samples.iter().copied().max().unwrap_or(0);
	let average_duration_seconds = samples.iter().sum::<u64>() as f64 / samples.len() as f64;
	let is_rust = is_rust_job_or_context(job, &[], root);
	let threshold_minutes = duration_threshold_minutes(
		is_rust,
		thresholds.max_duration_minutes,
		thresholds.rust_max_duration_minutes,
	);

	if !exceeds_duration_threshold(max_duration_seconds, threshold_minutes) {
		return None;
	}

	Some(CiSlowJobReport {
		job: job.to_string(),
		max_duration_seconds,
		average_duration_seconds,
		threshold_minutes,
		is_rust_job: is_rust,
		suggestions: build_speed_suggestions(job, is_rust, max_duration_seconds, threshold_minutes),
	})
}

/// Collect unique tuning advice from the incidents and slow jobs.
fn tuning_recommendations(
	tallies: &VerificationTallies,
	memory: &CiMemoryStore,
	slow_jobs: &[CiSlowJobReport],
) -> Vec<String> {
	let mut recommendations = Vec::new();

	for incident in &tallies.false_positive_incidents {
		push_unique(&mut recommendations, incident.recommendation.clone());
	}

	if !tallies.false_negative_incidents.is_empty() {
		recommendations.push(format!(
			"{} false-negative incident(s) detected; {} learned rule(s) are now active in `.monochange/ci-memory.json` to force `y` on matching paths.",
			tallies.false_negative_incidents.len(),
			memory.learned_rules.len()
		));
	}

	if !slow_jobs.is_empty() {
		recommendations.push(format!(
			"{} slow job(s) exceeded duration red-flag thresholds; apply caching and test sharding recommendations.",
			slow_jobs.len()
		));
	}

	recommendations
}

/// Append a recommendation only when it is not already present.
fn push_unique(values: &mut Vec<String>, value: String) {
	if !values.contains(&value) {
		values.push(value);
	}
}

/// Divide two counts, falling back when the denominator is zero.
fn ratio(numerator: usize, denominator: usize, fallback: f64) -> f64 {
	if denominator == 0 {
		return fallback;
	}

	numerator as f64 / denominator as f64
}
