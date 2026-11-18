//! The `ci history` subcommand.
//!
//! History is the protected read side of the memory store: it only answers for
//! the repository that owns the store and can be filtered by job or trimmed to
//! the most recent entries so a CI log stays readable.

use std::path::Path;

use clap::ArgMatches;
use monochange_core::MonochangeResult;

use super::args;
use super::auth;
use super::render::render_history_report;
use super::types::CiDecisionRecord;
use super::types::CiHistoryReport;
use super::types::CiMemoryStore;
use super::types::CiVerificationRecord;

/// Read and render the protected decision and verification history.
pub(super) fn run_ci_history_subcommand(
	root: &Path,
	matches: &ArgMatches,
) -> MonochangeResult<String> {
	let repo_id_override = args::repo_id_override(matches);
	let explicit_token = args::explicit_token(matches);

	// History always proves repository ownership before reading the store.
	let auth = auth::authenticate_repo_access(root, repo_id_override, explicit_token)?;
	let memory = auth::load_ci_memory_store(root, repo_id_override, true, explicit_token)?;
	let job_filter = matches.get_one::<String>("job").map(String::as_str);
	let limit = matches.get_one::<usize>("limit").copied();
	let report = CiHistoryReport {
		repo_id: auth.repo_id,
		repo_hash: auth.repo_hash,
		authenticated: true,
		learned_rules: memory.learned_rules.clone(),
		decisions: filter_decisions(&memory, job_filter, limit),
		verifications: filter_verifications(&memory, job_filter, limit),
	};

	render_history_report(&report, args::format_arg(matches))
}

/// Filter decisions by job and keep the most recent `limit` entries.
fn filter_decisions(
	memory: &CiMemoryStore,
	job_filter: Option<&str>,
	limit: Option<usize>,
) -> Vec<CiDecisionRecord> {
	let decisions = memory
		.decisions
		.iter()
		.filter(|decision| job_filter.is_none_or(|job| decision.job == job))
		.cloned()
		.collect();

	truncate_tail(decisions, limit)
}

/// Filter verifications by job and keep the most recent `limit` entries.
fn filter_verifications(
	memory: &CiMemoryStore,
	job_filter: Option<&str>,
	limit: Option<usize>,
) -> Vec<CiVerificationRecord> {
	let verifications = memory
		.verifications
		.iter()
		.filter(|verification| job_filter.is_none_or(|job| verification.job == job))
		.cloned()
		.collect();

	truncate_tail(verifications, limit)
}

/// Keep only the newest `limit` entries of a chronological list.
fn truncate_tail<T>(items: Vec<T>, limit: Option<usize>) -> Vec<T> {
	let Some(max_items) = limit else {
		return items;
	};

	if items.len() <= max_items {
		return items;
	}

	let split_index = items.len() - max_items;

	items.into_iter().skip(split_index).collect()
}
