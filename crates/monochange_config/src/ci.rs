//! `[ci]` configuration for the conditional CI gatekeeper.
//!
//! The `[ci]` table in `monochange.toml` is the single declarative source for
//! CI gate decisions: which jobs exist, which workspace packages each job
//! builds, and which contexts always run. The defaults encode the safe
//! direction — an unknown job runs, a provider may not skip a job the graph
//! wants to run, and toolchain changes trigger everything.

use std::collections::BTreeMap;
use std::path::Path;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Deserialize;

use super::load_raw_configuration;

/// Decision providers recognized by `[ci]` provider and `--provider`.
pub const CI_PROVIDERS: &[&str] = &[
	"graph",
	"opencode-go",
	"openrouter",
	"cloudflare",
	"jev",
	"openai-compatible",
];
/// Default provider: the deterministic workspace graph.
pub const DEFAULT_CI_PROVIDER: &str = "graph";
/// Default consecutive skips allowed on `main` before a forced run.
pub const DEFAULT_MAX_SKIPS_ON_MAIN: u32 = 5;
/// Events that always run every job regardless of the graph verdict.
pub const DEFAULT_UNCONDITIONAL_EVENTS: &[&str] = &["schedule", "workflow_dispatch"];
/// Branch and ref globs that always run every job.
pub const DEFAULT_UNCONDITIONAL_BRANCHES: &[&str] =
	&["release/*", "monochange/release/*", "refs/tags/v*"];
/// Toolchain paths that always run every job.
pub const DEFAULT_UNCONDITIONAL_PATHS: &[&str] =
	&["Cargo.lock", "rust-toolchain.toml", ".github/workflows/**"];
/// Changed paths that never trigger a job on their own.
pub const DEFAULT_IGNORE_PATHS: &[&str] = &["**/*.md", "docs/**"];

/// Resolved `[ci]` settings with every default applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiSettings {
	/// Decision provider used when `--provider` is omitted.
	pub provider: String,
	/// Optional model identifier for AI decision providers.
	pub model: Option<String>,
	/// Whether a provider verdict may skip a job the graph wants to run.
	pub ai_can_skip: bool,
	/// Consecutive skips allowed on `main` before a forced verification run.
	pub max_skips_on_main: u32,
	/// Events that force every job to run.
	pub unconditional_events: Vec<String>,
	/// Branch and ref globs that force every job to run.
	pub unconditional_branches: Vec<String>,
	/// Changed paths that force every job to run.
	pub unconditional_paths: Vec<String>,
	/// Changed paths that never trigger a job on their own.
	pub ignore_paths: Vec<String>,
	/// Configured jobs keyed by their GitHub job id.
	pub jobs: BTreeMap<String, CiJob>,
}

/// One `[ci.jobs.<id>]` entry: the roots a job builds plus non-Cargo inputs.
///
/// `packages` lists only the ROOT packages the job builds or tests; globs are
/// allowed and expand against every workspace package. Transitive, dev, and
/// build dependencies of a root also run the job, so a new crate matching a
/// root glob is covered with zero configuration edits. Table entries pin the
/// cargo feature selection the job actually builds so optional dependencies
/// only run the job when the selection activates them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CiJob {
	/// Root packages (ids, names, or globs) whose changes run this job.
	pub packages: Vec<CiJobPackage>,
	/// Non-package file globs whose changes run this job.
	pub paths: Vec<String>,
	/// Whether the job always runs regardless of the graph.
	pub always: bool,
}

/// The cargo feature selection a job builds a root package with.
///
/// The string form of a `packages` entry resolves to [`FeatureSelection::AllFeatures`]:
/// the conservative pre-selection behaviour that follows every edge, including
/// optional ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureSelection {
	/// Build with every feature; every dependency edge counts.
	AllFeatures,
	/// Build with the package's `default` feature (when it defines one) plus
	/// these features.
	DefaultsPlus(Vec<String>),
	/// Build with exactly these features, without the defaults.
	Only(Vec<String>),
}

/// One `[ci.jobs.<id>].packages` entry: a root package and the feature
/// selection the job builds it with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CiJobPackage {
	/// Root package id, name, or glob.
	pub name: String,
	/// Cargo feature selection the job builds this root with.
	pub selection: FeatureSelection,
}

impl CiJobPackage {
	/// Human-readable label used in decision reasons.
	///
	/// The all-features form keeps the bare name so string entries read
	/// exactly as before; feature-aware entries spell out their selection.
	#[must_use]
	pub fn label(&self) -> String {
		match &self.selection {
			FeatureSelection::AllFeatures => self.name.clone(),
			FeatureSelection::DefaultsPlus(features) => {
				if features.is_empty() {
					format!("{} (default features)", self.name)
				} else {
					format!("{} (default + {})", self.name, features.join(","))
				}
			}
			FeatureSelection::Only(features) => {
				if features.is_empty() {
					format!("{} (no default features)", self.name)
				} else {
					format!("{} ({})", self.name, features.join(","))
				}
			}
		}
	}
}

/// Raw `[ci]` table exactly as written in `monochange.toml`.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", schemars(rename = "ci_configuration"))]
pub(crate) struct RawCiConfiguration {
	/// Decision provider: graph, opencode-go, openrouter, cloudflare, jev, or
	/// openai-compatible. Defaults to `graph`.
	#[serde(default)]
	provider: Option<String>,
	/// Model identifier for AI decision providers.
	#[serde(default)]
	model: Option<String>,
	/// Whether a provider verdict may skip a job the graph wants to run.
	/// Defaults to `false`: providers may only turn `n` into `y`.
	#[serde(default)]
	ai_can_skip: bool,
	/// Consecutive skips allowed on `main` before a forced verification run.
	#[serde(default = "raw_max_skips_on_main")]
	max_skips_on_main: u32,
	/// Events that force every job to run.
	#[serde(default = "raw_unconditional_events")]
	unconditional_events: Vec<String>,
	/// Branch and ref globs that force every job to run.
	#[serde(default = "raw_unconditional_branches")]
	unconditional_branches: Vec<String>,
	/// Changed paths that force every job to run.
	#[serde(default = "raw_unconditional_paths")]
	unconditional_paths: Vec<String>,
	/// Changed paths that never trigger a job on their own.
	#[serde(default = "raw_ignore_paths")]
	ignore_paths: Vec<String>,
	/// Jobs keyed by their GitHub job id.
	#[serde(default)]
	jobs: BTreeMap<String, RawCiJob>,
}

impl Default for RawCiConfiguration {
	/// Defaults for a missing `[ci]` table must match the per-field serde
	/// defaults, so `#[serde(default)]` on the whole table stays consistent.
	fn default() -> Self {
		Self {
			provider: None,
			model: None,
			ai_can_skip: false,
			max_skips_on_main: raw_max_skips_on_main(),
			unconditional_events: raw_unconditional_events(),
			unconditional_branches: raw_unconditional_branches(),
			unconditional_paths: raw_unconditional_paths(),
			ignore_paths: raw_ignore_paths(),
			jobs: BTreeMap::new(),
		}
	}
}

/// Raw `[ci.jobs.<id>]` table.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", schemars(rename = "ci_job"))]
pub(crate) struct RawCiJob {
	/// Root packages (ids, names, or globs) whose changes run the job.
	#[serde(default)]
	packages: Vec<RawCiJobPackage>,
	/// Non-package file globs whose changes run the job.
	#[serde(default)]
	paths: Vec<String>,
	/// Whether the job always runs regardless of the graph.
	#[serde(default)]
	always: bool,
}

/// Raw `[ci.jobs.<id>].packages` entry: a bare name or a feature table.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(untagged))]
#[derive(Debug, Clone)]
pub(crate) enum RawCiJobPackage {
	/// Bare name: every feature, every edge (the conservative string form).
	Name(String),
	/// Table form pinning the cargo feature selection the job builds.
	Table(RawCiJobPackageTable),
}

impl<'de> Deserialize<'de> for RawCiJobPackage {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		struct RawCiJobPackageVisitor;

		impl<'de> serde::de::Visitor<'de> for RawCiJobPackageVisitor {
			type Value = RawCiJobPackage;

			fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
				formatter
					.write_str("a package name string or a feature-selection table with `name`")
			}

			fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
				Ok(RawCiJobPackage::Name(value.to_string()))
			}

			fn visit_map<A>(self, map: A) -> Result<Self::Value, A::Error>
			where
				A: serde::de::MapAccess<'de>,
			{
				let table = RawCiJobPackageTable::deserialize(
					serde::de::value::MapAccessDeserializer::new(map),
				)?;

				Ok(RawCiJobPackage::Table(table))
			}
		}

		deserializer.deserialize_any(RawCiJobPackageVisitor)
	}
}

/// Raw feature-selection table of one `packages` entry.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", schemars(rename = "ci_job_package"))]
pub(crate) struct RawCiJobPackageTable {
	/// Root package id, name, or glob.
	name: String,
	/// Extra features to enable on the root.
	#[serde(default)]
	features: Option<Vec<String>>,
	/// Whether the root's default feature is enabled; defaults to `true`.
	#[serde(default)]
	default_features: Option<bool>,
	/// Build with every feature; excludes `features` and `default_features`.
	#[serde(default)]
	all_features: bool,
}

/// Load `[ci]` settings from `monochange.toml`, applying defaults.
///
/// A missing `monochange.toml` yields the defaults; a malformed file or an
/// unknown provider is a configuration error because every decision after it
/// would silently use the wrong engine.
#[tracing::instrument(skip_all)]
pub fn load_ci_settings(root: &Path) -> MonochangeResult<CiSettings> {
	let (_, raw) = load_raw_configuration(root)?;

	resolve_ci_settings(raw.ci)
}

/// Apply defaults to a raw `[ci]` table and validate the provider.
fn resolve_ci_settings(raw: RawCiConfiguration) -> MonochangeResult<CiSettings> {
	let provider = raw
		.provider
		.unwrap_or_else(|| DEFAULT_CI_PROVIDER.to_string());

	if !CI_PROVIDERS.contains(&provider.as_str()) {
		return Err(MonochangeError::Config(format!(
			"unsupported `[ci]` provider `{provider}`; expected one of: {}",
			CI_PROVIDERS.join(", ")
		)));
	}

	Ok(CiSettings {
		provider,
		model: raw.model,
		ai_can_skip: raw.ai_can_skip,
		max_skips_on_main: raw.max_skips_on_main,
		unconditional_events: raw.unconditional_events,
		unconditional_branches: raw.unconditional_branches,
		unconditional_paths: raw.unconditional_paths,
		ignore_paths: raw.ignore_paths,
		jobs: raw
			.jobs
			.into_iter()
			.map(|(id, job)| {
				let packages = job
					.packages
					.into_iter()
					.map(|package| resolve_job_package(&id, package))
					.collect::<MonochangeResult<Vec<_>>>()?;

				Ok((
					id,
					CiJob {
						packages,
						paths: job.paths,
						always: job.always,
					},
				))
			})
			.collect::<MonochangeResult<BTreeMap<_, _>>>()?,
	})
}

/// Resolve one `packages` entry, rejecting contradictory selections.
///
/// `all_features = true` excludes `features` and `default_features`, and globs
/// must keep the all-features form because a glob expands to many packages
/// that share no feature set.
fn resolve_job_package(job_id: &str, package: RawCiJobPackage) -> MonochangeResult<CiJobPackage> {
	let (name, selection) = match package {
		RawCiJobPackage::Name(name) => (name, FeatureSelection::AllFeatures),
		RawCiJobPackage::Table(table) => {
			if table.all_features && (table.features.is_some() || table.default_features.is_some())
			{
				return Err(MonochangeError::Config(format!(
					"job `{job_id}` package `{}` sets `all_features = true` together with `features` or `default_features`; choose one form",
					table.name
				)));
			}

			let selection = if table.all_features {
				FeatureSelection::AllFeatures
			} else if table.default_features.unwrap_or(true) {
				FeatureSelection::DefaultsPlus(table.features.unwrap_or_default())
			} else {
				FeatureSelection::Only(table.features.unwrap_or_default())
			};

			(table.name, selection)
		}
	};

	if selection != FeatureSelection::AllFeatures && name.contains(['*', '?', '[', ']']) {
		return Err(MonochangeError::Config(format!(
			"job `{job_id}` package `{name}` combines a glob with a feature selection; globs always resolve to every feature"
		)));
	}

	Ok(CiJobPackage { name, selection })
}

/// Raw default for `max_skips_on_main`.
fn raw_max_skips_on_main() -> u32 {
	DEFAULT_MAX_SKIPS_ON_MAIN
}

/// Raw default for `unconditional_events`.
fn raw_unconditional_events() -> Vec<String> {
	DEFAULT_UNCONDITIONAL_EVENTS
		.iter()
		.map(|value| (*value).to_string())
		.collect()
}

/// Raw default for `unconditional_branches`.
fn raw_unconditional_branches() -> Vec<String> {
	DEFAULT_UNCONDITIONAL_BRANCHES
		.iter()
		.map(|value| (*value).to_string())
		.collect()
}

/// Raw default for `unconditional_paths`.
fn raw_unconditional_paths() -> Vec<String> {
	DEFAULT_UNCONDITIONAL_PATHS
		.iter()
		.map(|value| (*value).to_string())
		.collect()
}

/// Raw default for `ignore_paths`.
fn raw_ignore_paths() -> Vec<String> {
	DEFAULT_IGNORE_PATHS
		.iter()
		.map(|value| (*value).to_string())
		.collect()
}

#[cfg(test)]
#[path = "__tests__/ci_tests.rs"]
mod tests;
