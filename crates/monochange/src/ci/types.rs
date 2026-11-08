//! Serialized data model for the CI gatekeeper.
//!
//! Every struct here is part of the on-disk `.monochange/ci-memory.json`
//! contract or the JSON output surface consumed by CI workflows, so field names
//! and serde attributes are intentionally explicit.

use std::collections::BTreeMap;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Deserialize;
use serde::Serialize;

use super::compute_repo_hash;

/// Repository-scoped CI auth record persisted at `.monochange/ci-auth.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiAuthFile {
	pub schema_version: u32,
	pub repo_id: String,
	pub repo_hash: String,
	pub secret: String,
	pub token: String,
	pub created_at: String,
}

/// Result of an auth `init`, `verify`, or `token` call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiAuthReport {
	pub action: String,
	pub authenticated: bool,
	pub repo_id: String,
	pub repo_hash: String,
	pub token: String,
	pub auth_file: String,
	pub source: String,
}

/// Strategy used to decide whether a CI job should run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CiDecisionProvider {
	OpencodeGo,
	Openrouter,
	Cloudflare,
	Jev,
	OpenaiCompatible,
	Graph,
}

impl CiDecisionProvider {
	/// Parse a CLI provider name, rejecting anything outside the supported set.
	pub fn parse(value: &str) -> MonochangeResult<Self> {
		match value.trim() {
			"opencode-go" => Ok(Self::OpencodeGo),
			"openrouter" => Ok(Self::Openrouter),
			"cloudflare" => Ok(Self::Cloudflare),
			"jev" => Ok(Self::Jev),
			"openai-compatible" => Ok(Self::OpenaiCompatible),
			"graph" => Ok(Self::Graph),
			other => {
				Err(MonochangeError::Config(format!(
					"unsupported CI decision provider `{other}`; expected one of: opencode-go, openrouter, cloudflare, jev, openai-compatible, graph"
				)))
			}
		}
	}

	/// Stable provider name used in JSON records and CLI output.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::OpencodeGo => "opencode-go",
			Self::Openrouter => "openrouter",
			Self::Cloudflare => "cloudflare",
			Self::Jev => "jev",
			Self::OpenaiCompatible => "openai-compatible",
			Self::Graph => "graph",
		}
	}
}

/// Closed-loop classification of a decision against the real CI outcome.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CiVerificationClassification {
	TruePositive,
	TrueNegative,
	FalsePositive,
	FalseNegative,
}

impl CiVerificationClassification {
	/// Stable classification name used in analytics output.
	pub fn as_str(self) -> &'static str {
		match self {
			Self::TruePositive => "true_positive",
			Self::TrueNegative => "true_negative",
			Self::FalsePositive => "false_positive",
			Self::FalseNegative => "false_negative",
		}
	}
}

/// Learned override created when a skipped job later failed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CiLearnedRule {
	pub id: String,
	pub job: String,
	pub patterns: Vec<String>,
	#[serde(default)]
	pub packages: Vec<String>,
	pub reason: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_commit: Option<String>,
	pub created_at: String,
}

/// One recorded gate decision, including the evidence behind it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiDecisionRecord {
	pub id: String,
	pub repo_id: String,
	pub job: String,
	pub decision: String,
	pub run: bool,
	pub unconditional: bool,
	pub provider: String,
	pub model: String,
	pub reason: String,
	#[serde(default)]
	pub unconditional_reasons: Vec<String>,
	#[serde(default)]
	pub matched_learned_rules: Vec<String>,
	#[serde(default)]
	pub affected_packages: Vec<String>,
	#[serde(default)]
	pub changed_files: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub event: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub branch: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub commit: Option<String>,
	pub consecutive_skips_on_main: u32,
	pub duration_red_flag: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub observed_duration_seconds: Option<u64>,
	pub threshold_minutes: u64,
	#[serde(default)]
	pub speed_suggestions: Vec<String>,
	pub timestamp: String,
}

/// One recorded verification of a decision against a real CI outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiVerificationRecord {
	pub id: String,
	pub repo_id: String,
	pub job: String,
	pub conclusion: String,
	pub duration_seconds: u64,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub commit: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub pr: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub branch: Option<String>,
	pub prior_decision: String,
	pub classification: CiVerificationClassification,
	pub reason: String,
	#[serde(default)]
	pub changed_files: Vec<String>,
	#[serde(default)]
	pub affected_packages: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub learned_rule_added: Option<CiLearnedRule>,
	pub duration_red_flag: bool,
	pub threshold_minutes: u64,
	#[serde(default)]
	pub speed_suggestions: Vec<String>,
	pub timestamp: String,
}

/// Repository-scoped memory store persisted at `.monochange/ci-memory.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiMemoryStore {
	pub schema_version: u32,
	pub repo_id: String,
	pub repo_hash: String,
	#[serde(default)]
	pub consecutive_skips_on_main: BTreeMap<String, u32>,
	#[serde(default)]
	pub learned_rules: Vec<CiLearnedRule>,
	#[serde(default)]
	pub decisions: Vec<CiDecisionRecord>,
	#[serde(default)]
	pub verifications: Vec<CiVerificationRecord>,
}

impl CiMemoryStore {
	/// Create an empty store for `repo_id`.
	pub fn new(repo_id: String) -> Self {
		let repo_hash = compute_repo_hash(&repo_id);

		Self {
			schema_version: 1,
			repo_id,
			repo_hash,
			consecutive_skips_on_main: BTreeMap::new(),
			learned_rules: Vec::new(),
			decisions: Vec::new(),
			verifications: Vec::new(),
		}
	}
}

/// Aggregate gate decision for every job evaluated in one invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiDecideReport {
	pub repo_id: String,
	pub decision: String,
	pub run: bool,
	pub unconditional: bool,
	pub provider: String,
	pub model: String,
	pub reason: String,
	pub jobs: Vec<CiDecisionRecord>,
}

/// Duration summary for a job that exceeded its red-flag threshold.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiSlowJobReport {
	pub job: String,
	pub max_duration_seconds: u64,
	pub average_duration_seconds: f64,
	pub threshold_minutes: u64,
	pub is_rust_job: bool,
	pub suggestions: Vec<String>,
}

/// A skipped decision that should have run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiFalseNegativeIncident {
	pub id: String,
	pub job: String,
	pub commit: Option<String>,
	pub pr: Option<String>,
	pub changed_files: Vec<String>,
	pub reason: String,
	pub learned_rule: Option<CiLearnedRule>,
	pub timestamp: String,
}

/// A run that should have been skipped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiFalsePositiveIncident {
	pub id: String,
	pub job: String,
	pub commit: Option<String>,
	pub pr: Option<String>,
	pub changed_files: Vec<String>,
	pub wasted_vm_minutes: f64,
	pub reason: String,
	pub recommendation: String,
	pub timestamp: String,
}

/// Protected analytics report for a single repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiAnalyticsReport {
	pub repo_id: String,
	pub repo_hash: String,
	pub authenticated: bool,
	pub total_decisions: usize,
	pub run_decisions: usize,
	pub skip_decisions: usize,
	pub unconditional_decisions: usize,
	pub skip_rate: f64,
	pub estimated_vm_minutes_saved: f64,
	pub wasted_vm_minutes: f64,
	pub true_positives: usize,
	pub true_negatives: usize,
	pub false_positives: usize,
	pub false_negatives: usize,
	pub precision: f64,
	pub recall: f64,
	pub false_negative_rate: f64,
	pub learned_rules_count: usize,
	pub false_negative_incidents: Vec<CiFalseNegativeIncident>,
	pub false_positive_incidents: Vec<CiFalsePositiveIncident>,
	pub tuning_recommendations: Vec<String>,
	pub slow_jobs: Vec<CiSlowJobReport>,
}

/// Protected decision and verification history for a single repository.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiHistoryReport {
	pub repo_id: String,
	pub repo_hash: String,
	pub authenticated: bool,
	pub learned_rules: Vec<CiLearnedRule>,
	pub decisions: Vec<CiDecisionRecord>,
	pub verifications: Vec<CiVerificationRecord>,
}
