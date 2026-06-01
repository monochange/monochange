use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

use crate::BumpSeverity;
use crate::Ecosystem;
use crate::MonochangeResult;
use crate::PackageRecord;

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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct PackageSnapshotFile {
	/// Package-relative path.
	pub path: PathBuf,
	/// UTF-8-decoded file contents.
	pub contents: String,
}

/// A package snapshot at one side of the comparison.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageSnapshot {
	/// Human-readable label for this snapshot.
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

/// One semantic diff record emitted by an ecosystem analyzer.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
}

/// Analyzer output for one package.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
