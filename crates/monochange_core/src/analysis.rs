use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use glob::Pattern;
use serde::Deserialize;
use serde::Serialize;

use crate::BumpSeverity;
use crate::Ecosystem;
use crate::MonochangeResult;
use crate::PackageRecord;

/// How a repository-relative path relates to one configured package.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum PackagePathMatch {
	/// The path affects the package.
	Touched,
	/// The path is inside the package but excluded by its ignored paths.
	Ignored,
	/// The path does not belong to the package.
	Unmatched,
}

/// Compiled path policy for one configured package.
#[derive(Debug, Clone)]
pub struct PackagePathMatcher {
	package_id: String,
	package_root: String,
	package_root_prefix: String,
	additional_patterns: Vec<Pattern>,
	ignored_patterns: Vec<Pattern>,
}

impl PackagePathMatcher {
	/// Build a matcher from a package's configured path policy.
	#[must_use]
	pub fn new(
		package_id: impl Into<String>,
		package_root: &Path,
		additional_paths: &[String],
		ignored_paths: &[String],
	) -> Self {
		let package_root = normalize_repository_path(&package_root.to_string_lossy());
		let package_root_prefix = format!("{package_root}/");

		Self {
			package_id: package_id.into(),
			package_root,
			package_root_prefix,
			additional_patterns: compile_path_patterns(additional_paths),
			ignored_patterns: compile_path_patterns(ignored_paths),
		}
	}

	/// Return the configured package id represented by this matcher.
	#[must_use]
	pub fn package_id(&self) -> &str {
		&self.package_id
	}

	/// Classify one repository-relative path.
	#[must_use]
	pub fn classify(&self, path: &Path) -> PackagePathMatch {
		let path = normalize_repository_path(&path.to_string_lossy());
		let relative_path =
			package_relative_path(&path, &self.package_root, &self.package_root_prefix);

		if matches_any_package_pattern(&path, relative_path, &self.additional_patterns) {
			return PackagePathMatch::Touched;
		}

		if relative_path.is_none() {
			return PackagePathMatch::Unmatched;
		}

		if matches_any_package_pattern(&path, relative_path, &self.ignored_patterns) {
			return PackagePathMatch::Ignored;
		}

		PackagePathMatch::Touched
	}
}

fn normalize_repository_path(path: &str) -> String {
	let normalized = path.trim().replace('\\', "/");
	let normalized = normalized.trim_start_matches("./");
	normalized.trim_matches('/').to_string()
}

fn compile_path_patterns(patterns: &[String]) -> Vec<Pattern> {
	patterns
		.iter()
		.filter_map(|pattern| Pattern::new(pattern).ok())
		.collect()
}

fn package_relative_path<'path>(
	path: &'path str,
	package_root: &str,
	package_root_prefix: &str,
) -> Option<&'path str> {
	if package_root.is_empty() {
		return Some(path);
	}
	path.strip_prefix(package_root_prefix)
		.or_else(|| (path == package_root).then_some(""))
}

fn matches_any_package_pattern(
	path: &str,
	relative_path: Option<&str>,
	patterns: &[Pattern],
) -> bool {
	patterns.iter().any(|pattern| {
		pattern.matches(path)
			|| relative_path.is_some_and(|relative_path| pattern.matches(relative_path))
	})
}

/// Level of detail requested from semantic analyzers.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DetectionLevel {
	/// Fastest mode. Prefer lightweight structural extraction.
	Basic,
	/// Extract before/after signatures when possible.
	Signature,
	/// Perform the richest semantic extraction available for the ecosystem.
	Semantic,
}

/// How a file changed between the analyzed revisions.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FileChangeKind {
	Added,
	Modified,
	Deleted,
}

/// One file that changed for the analyzed package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AnalyzedFileChange {
	/// Repository-relative path.
	pub path: PathBuf,
	/// Package-relative path.
	pub package_path: PathBuf,
	/// Change kind.
	pub kind: FileChangeKind,
	/// File contents before the change, when available and text-decodable.
	pub before_contents: Option<String>,
	/// File contents after the change, when available and text-decodable.
	pub after_contents: Option<String>,
}

/// One text file captured in a package snapshot.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackageSnapshotFile {
	/// Package-relative path.
	pub path: PathBuf,
	/// UTF-8-decoded file contents.
	pub contents: String,
}

/// A package snapshot at one side of the comparison.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackageSnapshot {
	/// Stable label for this snapshot, normally an immutable Git tree id.
	pub label: String,
	/// Text files available to analyzers.
	pub files: Vec<PackageSnapshotFile>,
}

impl PackageSnapshot {
	/// Look up one file by package-relative path.
	#[must_use]
	pub fn file(&self, path: &Path) -> Option<&PackageSnapshotFile> {
		self.files.iter().find(|file| file.path == path)
	}
}

/// Stable schema version for monochange API snapshot files.
pub const API_SNAPSHOT_SCHEMA_VERSION: u16 = 1;

/// Normalized package API surface captured by monochange.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiSnapshot {
	/// Snapshot file schema version.
	pub schema_version: u16,
	/// Package identifier used in reports.
	pub package_id: String,
	/// Human-readable package name.
	pub package_name: String,
	/// Package ecosystem.
	pub ecosystem: Ecosystem,
	/// Analyzer that produced the snapshot.
	pub analyzer_id: String,
	/// Deterministically sorted public API items.
	pub items: Vec<ApiItem>,
	/// Non-fatal extraction warnings.
	pub warnings: Vec<String>,
}

impl ApiSnapshot {
	/// Create an API snapshot and sort its items for stable serialization and diffing.
	#[must_use]
	pub fn new(
		package_id: impl Into<String>,
		package_name: impl Into<String>,
		ecosystem: Ecosystem,
		analyzer_id: impl Into<String>,
		items: Vec<ApiItem>,
		warnings: Vec<String>,
	) -> Self {
		let mut snapshot = Self {
			schema_version: API_SNAPSHOT_SCHEMA_VERSION,
			package_id: package_id.into(),
			package_name: package_name.into(),
			ecosystem,
			analyzer_id: analyzer_id.into(),
			items,
			warnings,
		};
		snapshot.sort_items();
		snapshot
	}

	/// Sort items by stable id for deterministic output.
	pub fn sort_items(&mut self) {
		self.items.sort_by(|left, right| left.id.cmp(&right.id));
	}

	/// Diff two API snapshots keyed by stable item id.
	#[must_use]
	pub fn diff(&self, after: &Self) -> ApiDiff {
		diff_api_snapshots(self, after)
	}
}

/// One normalized public API item.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiItem {
	/// Stable id unique within a package snapshot.
	pub id: String,
	/// Ecosystem-specific kind such as `function`, `export`, `bin`, or `dependency`.
	pub kind: String,
	/// Human-facing item path.
	pub path: String,
	/// Signature or descriptor, when available.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub signature: Option<String>,
	/// Package-relative source path that contributed evidence, when available.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub source_path: Option<PathBuf>,
	/// Additional stable item metadata.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub metadata: BTreeMap<String, String>,
}

impl ApiItem {
	/// Create an API item using a conventional `<kind>:<path>` id.
	#[must_use]
	pub fn new(
		kind: impl Into<String>,
		path: impl Into<String>,
		signature: Option<String>,
	) -> Self {
		let kind = kind.into();
		let path = path.into();
		Self {
			id: format!("{kind}:{path}"),
			kind,
			path,
			signature,
			source_path: None,
			metadata: BTreeMap::new(),
		}
	}

	/// Attach a package-relative source path.
	#[must_use]
	pub fn with_source_path(mut self, source_path: impl Into<PathBuf>) -> Self {
		self.source_path = Some(source_path.into());
		self
	}
}

/// Confidence level for API impact classification.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiConfidence {
	Low,
	Medium,
	High,
}

/// API item change kind.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApiChangeKind {
	Added,
	Removed,
	Modified,
}

/// One normalized API diff entry.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiChange {
	pub kind: ApiChangeKind,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub before: Option<ApiItem>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub after: Option<ApiItem>,
	pub suggested_bump: BumpSeverity,
	pub confidence: ApiConfidence,
	pub summary: String,
}

/// Diff between two normalized API snapshots.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiDiff {
	pub package_id: String,
	pub package_name: String,
	pub ecosystem: Ecosystem,
	pub analyzer_id: String,
	pub suggested_bump: BumpSeverity,
	pub changes: Vec<ApiChange>,
	pub warnings: Vec<String>,
}

impl ApiDiff {
	/// Return true when the diff has no item-level changes.
	#[must_use]
	pub fn is_empty(&self) -> bool {
		self.changes.is_empty()
	}
}

/// Diff normalized API snapshots keyed by `ApiItem.id`.
#[must_use]
pub fn diff_api_snapshots(before: &ApiSnapshot, after: &ApiSnapshot) -> ApiDiff {
	let before_items: BTreeMap<_, _> = before.items.iter().map(|item| (&item.id, item)).collect();
	let after_items: BTreeMap<_, _> = after.items.iter().map(|item| (&item.id, item)).collect();
	let mut changes = Vec::new();

	for (id, before_item) in &before_items {
		match after_items.get(id) {
			Some(after_item)
				if before_item.signature != after_item.signature
					|| before_item.metadata != after_item.metadata =>
			{
				changes.push(api_change_modified(before_item, after_item));
			}
			Some(_) => {}
			None => changes.push(api_change_removed(before_item)),
		}
	}

	for (id, after_item) in &after_items {
		if !before_items.contains_key(id) {
			changes.push(api_change_added(after_item));
		}
	}

	changes.sort_by(|left, right| left.summary.cmp(&right.summary));
	let suggested_bump = changes
		.iter()
		.map(|change| change.suggested_bump)
		.max()
		.unwrap_or(BumpSeverity::None);
	let warnings = before
		.warnings
		.iter()
		.chain(after.warnings.iter())
		.cloned()
		.collect();

	ApiDiff {
		package_id: after.package_id.clone(),
		package_name: after.package_name.clone(),
		ecosystem: after.ecosystem,
		analyzer_id: after.analyzer_id.clone(),
		suggested_bump,
		changes,
		warnings,
	}
}

fn api_change_added(item: &ApiItem) -> ApiChange {
	ApiChange {
		kind: ApiChangeKind::Added,
		before: None,
		after: Some(item.clone()),
		suggested_bump: BumpSeverity::Minor,
		confidence: ApiConfidence::High,
		summary: format!("added public {} `{}`", item.kind, item.path),
	}
}

fn api_change_removed(item: &ApiItem) -> ApiChange {
	ApiChange {
		kind: ApiChangeKind::Removed,
		before: Some(item.clone()),
		after: None,
		suggested_bump: BumpSeverity::Major,
		confidence: ApiConfidence::High,
		summary: format!("removed public {} `{}`", item.kind, item.path),
	}
}

fn api_change_modified(before: &ApiItem, after: &ApiItem) -> ApiChange {
	ApiChange {
		kind: ApiChangeKind::Modified,
		before: Some(before.clone()),
		after: Some(after.clone()),
		suggested_bump: BumpSeverity::Major,
		confidence: ApiConfidence::High,
		summary: format!("changed public {} `{}`", after.kind, after.path),
	}
}

/// High-level semantic change category shared across ecosystems.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticChangeCategory {
	Package,
	PublicApi,
	Export,
	Dependency,
	Metadata,
}

/// Whether an entity was added, removed, or modified.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticChangeKind {
	Added,
	Removed,
	Modified,
}

/// Compatibility outcome proven or inferred by a semantic analyzer.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticAnalysisOutcome {
	/// The analyzer could not reach a conclusive compatibility result.
	Inconclusive,
	/// The public contract remains compatible.
	Compatible,
	/// The public contract gained compatible capability.
	Additive,
	/// The public contract is incompatible with existing consumers.
	Breaking,
}

/// How much of the declared public surface an analyzer checked.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticAnalysisCompleteness {
	/// Every public surface in the analyzer's declared scope was checked.
	Complete,
	/// Some public surfaces or required inputs were unavailable.
	Partial,
	/// The package does not expose a surface supported by the analyzer.
	Unsupported,
}

/// Cargo feature selection used by one cargo-semver-checks matrix cell.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Copy, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CargoSemverFeatureMode {
	/// Check the crate-defined default features plus explicitly listed features.
	#[default]
	Default,
	/// Check every declared feature.
	All,
	/// Disable implicit features and use only explicitly listed features.
	None,
	/// Use cargo-semver-checks' stable-feature heuristic.
	Heuristic,
}

/// One configured cargo-semver-checks feature and target combination.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoSemverMatrixCell {
	/// Stable, human-readable identifier used in reports.
	pub name: String,
	/// Implicit feature-selection mode shared by both endpoints.
	#[serde(default)]
	pub feature_mode: CargoSemverFeatureMode,
	/// Extra features enabled at both endpoints.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub features: Vec<String>,
	/// Extra features enabled only for the baseline endpoint.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub baseline_features: Vec<String>,
	/// Extra features enabled only for the candidate endpoint.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub current_features: Vec<String>,
	/// Rust compilation target, or the host target when omitted.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub target: Option<String>,
}

impl Default for CargoSemverMatrixCell {
	fn default() -> Self {
		Self {
			name: "default".to_string(),
			feature_mode: CargoSemverFeatureMode::Default,
			features: Vec::new(),
			baseline_features: Vec::new(),
			current_features: Vec::new(),
			target: None,
		}
	}
}

fn default_cargo_semver_timeout_seconds() -> u64 {
	300
}

fn default_cargo_semver_matrix() -> Vec<CargoSemverMatrixCell> {
	vec![CargoSemverMatrixCell::default()]
}

/// Opt-in cargo-semver-checks configuration for semantic Rust analysis.
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CargoSemverChecksSettings {
	/// Run the external analyzer when semantic detection is requested.
	#[serde(default)]
	pub enabled: bool,
	/// Maximum duration of each matrix cell.
	#[serde(default = "default_cargo_semver_timeout_seconds")]
	pub timeout_seconds: u64,
	/// Feature and target combinations that define supported Rust API coverage.
	#[serde(default = "default_cargo_semver_matrix")]
	pub matrix: Vec<CargoSemverMatrixCell>,
}

impl Default for CargoSemverChecksSettings {
	fn default() -> Self {
		Self {
			enabled: false,
			timeout_seconds: default_cargo_semver_timeout_seconds(),
			matrix: default_cargo_semver_matrix(),
		}
	}
}

/// Completion state for one analyzer sub-check.
#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SemanticAnalyzerCheckStatus {
	/// The analyzer completed and returned a compatibility outcome.
	Checked,
	/// The analyzer deliberately skipped this scope because a prerequisite was unavailable.
	Skipped,
	/// The analyzer attempted this scope but did not produce trustworthy evidence.
	Failed,
}

/// One stable diagnostic emitted by a semantic analyzer sub-check.
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SemanticAnalyzerDiagnostic {
	/// Stable tool-specific diagnostic code.
	pub code: String,
	/// Human-readable diagnostic title or summary.
	pub message: String,
	/// Authoritative reference for the diagnostic, when available.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub reference: Option<String>,
}

impl SemanticAnalyzerDiagnostic {
	/// Create a diagnostic without an external reference.
	#[must_use]
	pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
		Self {
			code: code.into(),
			message: message.into(),
			reference: None,
		}
	}

	/// Attach an authoritative reference for this diagnostic.
	#[must_use]
	pub fn with_reference(mut self, reference: impl Into<String>) -> Self {
		self.reference = Some(reference.into());
		self
	}
}

/// Machine-readable result for one scope within an analyzer run.
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SemanticAnalyzerCheck {
	/// Stable check name, such as a feature/target matrix cell id.
	pub name: String,
	/// Whether the check completed, was skipped, or failed.
	pub status: SemanticAnalyzerCheckStatus,
	/// Exact analyzer-specific inputs used for this check.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub configuration: BTreeMap<String, String>,
	/// Compatibility outcome for a completed check.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub outcome: Option<SemanticAnalysisOutcome>,
	/// Minimum release bump reported by a completed check.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub suggested_bump: Option<BumpSeverity>,
	/// Stable diagnostics that explain the result.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub diagnostics: Vec<SemanticAnalyzerDiagnostic>,
}

impl SemanticAnalyzerCheck {
	/// Create a sub-check with no compatibility result or diagnostics.
	#[must_use]
	pub fn new(
		name: impl Into<String>,
		status: SemanticAnalyzerCheckStatus,
		configuration: BTreeMap<String, String>,
	) -> Self {
		Self {
			name: name.into(),
			status,
			configuration,
			outcome: None,
			suggested_bump: None,
			diagnostics: Vec::new(),
		}
	}

	/// Attach a completed compatibility result.
	#[must_use]
	pub fn with_result(
		mut self,
		outcome: SemanticAnalysisOutcome,
		suggested_bump: BumpSeverity,
	) -> Self {
		self.outcome = Some(outcome);
		self.suggested_bump = Some(suggested_bump);
		self
	}

	/// Attach diagnostics produced by this sub-check.
	#[must_use]
	pub fn with_diagnostics(mut self, diagnostics: Vec<SemanticAnalyzerDiagnostic>) -> Self {
		self.diagnostics = diagnostics;
		self
	}
}

/// Provenance and coverage for one analyzer assessment.
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SemanticAnalyzerEvidence {
	/// Stable analyzer identifier, such as `npm/typescript`.
	pub analyzer_id: String,
	/// Tool or engine that produced the evidence.
	pub engine: String,
	/// Engine version, when it could be resolved.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,
	/// Completeness of this assessment's declared coverage.
	pub completeness: SemanticAnalysisCompleteness,
	/// Human-readable description of the surface that was checked.
	pub coverage: String,
	/// Reason the primary analysis could not complete, when applicable.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub fallback_reason: Option<String>,
	/// Individual scopes checked by the analyzer, when it exposes them.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub checks: Vec<SemanticAnalyzerCheck>,
}

impl SemanticAnalyzerEvidence {
	/// Create analyzer evidence for a declared coverage boundary.
	pub fn new(
		analyzer_id: impl Into<String>,
		engine: impl Into<String>,
		completeness: SemanticAnalysisCompleteness,
		coverage: impl Into<String>,
	) -> Self {
		Self {
			analyzer_id: analyzer_id.into(),
			engine: engine.into(),
			version: None,
			completeness,
			coverage: coverage.into(),
			fallback_reason: None,
			checks: Vec::new(),
		}
	}

	/// Attach the engine version that produced this evidence.
	#[must_use]
	pub fn with_version(mut self, version: impl Into<String>) -> Self {
		self.version = Some(version.into());
		self
	}

	/// Attach the reason the primary analysis could not complete.
	#[must_use]
	pub fn with_fallback_reason(mut self, reason: impl Into<String>) -> Self {
		self.fallback_reason = Some(reason.into());
		self
	}

	/// Attach the analyzer's individual sub-check results.
	#[must_use]
	pub fn with_checks(mut self, checks: Vec<SemanticAnalyzerCheck>) -> Self {
		self.checks = checks;
		self
	}
}

/// Explicit compatibility and release recommendation from an analyzer.
#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SemanticChangeAssessment {
	/// Compatibility outcome produced by the analyzer.
	pub outcome: SemanticAnalysisOutcome,
	/// Changeset bump proposed from the analyzed evidence.
	pub suggested_bump: BumpSeverity,
	/// Confidence in the outcome.
	pub confidence: ApiConfidence,
	/// Analyzer provenance and coverage.
	pub evidence: SemanticAnalyzerEvidence,
}

impl SemanticChangeAssessment {
	/// Create an explicit compatibility and release assessment.
	#[must_use]
	pub fn new(
		outcome: SemanticAnalysisOutcome,
		suggested_bump: BumpSeverity,
		confidence: ApiConfidence,
		evidence: SemanticAnalyzerEvidence,
	) -> Self {
		Self {
			outcome,
			suggested_bump,
			confidence,
			evidence,
		}
	}
}

/// One semantic diff record emitted by an ecosystem analyzer.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub struct SemanticChange {
	/// Broad category of change.
	pub category: SemanticChangeCategory,
	/// Whether the item was added, removed, or modified.
	pub kind: SemanticChangeKind,
	/// Ecosystem-specific item kind such as `function`, `struct`, `class`, or `dependency`.
	pub item_kind: String,
	/// Stable symbol or item path, such as `crate::api::render` or `serde`.
	pub item_path: String,
	/// Human-readable explanation of the change.
	pub summary: String,
	/// Package-relative file path that contributed the evidence.
	pub file_path: PathBuf,
	/// Signature or descriptor before the change, when available.
	pub before_signature: Option<String>,
	/// Signature or descriptor after the change, when available.
	pub after_signature: Option<String>,
	/// Analyzer-provided compatibility assessment, when stronger evidence is available.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub assessment: Option<SemanticChangeAssessment>,
}

impl SemanticChange {
	/// Create one semantic change without optional signatures or an assessment.
	pub fn new(
		category: SemanticChangeCategory,
		kind: SemanticChangeKind,
		item_kind: impl Into<String>,
		item_path: impl Into<String>,
		summary: impl Into<String>,
		file_path: impl Into<PathBuf>,
	) -> Self {
		Self {
			category,
			kind,
			item_kind: item_kind.into(),
			item_path: item_path.into(),
			summary: summary.into(),
			file_path: file_path.into(),
			before_signature: None,
			after_signature: None,
			assessment: None,
		}
	}

	/// Attach the signature that existed before this change.
	#[must_use]
	pub fn with_before_signature(mut self, signature: impl Into<String>) -> Self {
		self.before_signature = Some(signature.into());
		self
	}

	/// Attach the signature that exists after this change.
	#[must_use]
	pub fn with_after_signature(mut self, signature: impl Into<String>) -> Self {
		self.after_signature = Some(signature.into());
		self
	}

	/// Attach analyzer-provided compatibility evidence.
	#[must_use]
	pub fn with_assessment(mut self, assessment: SemanticChangeAssessment) -> Self {
		self.assessment = Some(assessment);
		self
	}
}

/// Analyzer output for one package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PackageAnalysisResult {
	/// Unique analyzer identifier.
	pub analyzer_id: String,
	/// Package identifier used in reports.
	pub package_id: String,
	/// Package ecosystem.
	pub ecosystem: Ecosystem,
	/// Package-relative files that contributed to the analysis.
	pub changed_files: Vec<PathBuf>,
	/// Structured semantic diffs.
	pub semantic_changes: Vec<SemanticChange>,
	/// Non-fatal warnings from the analyzer.
	pub warnings: Vec<String>,
}

/// Input context passed to an ecosystem analyzer.
#[derive(Debug)]
pub struct PackageAnalysisContext<'a> {
	/// Repository root.
	pub repo_root: &'a Path,
	/// Discovered package being analyzed.
	pub package: &'a PackageRecord,
	/// Requested detection level.
	pub detection_level: DetectionLevel,
	/// File deltas for this package.
	pub changed_files: &'a [AnalyzedFileChange],
	/// Package snapshot before the change, when available.
	pub before_snapshot: Option<&'a PackageSnapshot>,
	/// Package snapshot after the change, when available.
	pub after_snapshot: Option<&'a PackageSnapshot>,
}

impl PackageAnalysisContext<'_> {
	/// Return the package root directory.
	#[must_use]
	pub fn package_root(&self) -> &Path {
		self.package
			.manifest_path
			.parent()
			.unwrap_or(&self.package.workspace_root)
	}
}

/// Ecosystem-specific semantic analyzer contract.
pub trait SemanticAnalyzer: Send + Sync {
	/// Stable analyzer identifier.
	fn analyzer_id(&self) -> &'static str;

	/// Return `true` when this analyzer can handle the package.
	fn applies_to(&self, package: &PackageRecord) -> bool;

	/// Analyze one package and return semantic diffs.
	fn analyze_package(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> MonochangeResult<PackageAnalysisResult>;
}

#[cfg(test)]
#[path = "__tests__/analysis_tests.rs"]
mod tests;
