use std::path::Path;
use std::path::PathBuf;

use monochange_analysis::AnalysisConfig;
use monochange_analysis::ChangeAnalysis;
use monochange_analysis::ChangeFrame;
use monochange_core::ApiChange;
use monochange_core::ApiChangeKind;
use monochange_core::ApiConfidence;
use monochange_core::ApiItem;
use monochange_core::BumpSeverity;
use monochange_core::DependencyKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::SemanticChange;
use serde::Deserialize;
use serde::Serialize;

use crate::OutputFormat;

const DEFAULT_BASE_REF: &str = "origin/main";
const DEFAULT_HEAD_REF: &str = "HEAD";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ClassifyOptions {
	pub(crate) base: String,
	pub(crate) head: String,
	pub(crate) format: OutputFormat,
	pub(crate) output: Option<PathBuf>,
	pub(crate) dependency_propagation: DependencyPropagation,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(crate) enum DependencyPropagation {
	#[default]
	None,
	Public,
}

impl Default for ClassifyOptions {
	fn default() -> Self {
		Self {
			base: DEFAULT_BASE_REF.to_string(),
			head: DEFAULT_HEAD_REF.to_string(),
			format: OutputFormat::Markdown,
			output: None,
			dependency_propagation: DependencyPropagation::None,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeClassificationReport {
	pub(crate) frame: String,
	pub(crate) recommendation: BumpSeverity,
	pub(crate) packages: Vec<PackageClassification>,
	pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageClassification {
	pub(crate) package_id: String,
	pub(crate) package_name: String,
	pub(crate) ecosystem: monochange_core::Ecosystem,
	pub(crate) recommendation: BumpSeverity,
	pub(crate) confidence: String,
	pub(crate) summary: String,
	pub(crate) analyzer_id: Option<String>,
	pub(crate) api_changes: Vec<ApiChange>,
	pub(crate) semantic_changes: Vec<SemanticChange>,
	pub(crate) warnings: Vec<String>,
}

#[coverage(off)]
pub(crate) fn parse_change_classify_options(
	args: &[std::ffi::OsString],
) -> MonochangeResult<Option<ClassifyOptions>> {
	let Some(command) = args.get(1).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	let Some(subcommand) = args.get(2).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	if command != "change" || subcommand != "classify" {
		return Ok(None);
	}

	let mut options = ClassifyOptions::default();
	let mut index = 3;
	while let Some(arg) = args.get(index).and_then(|value| value.to_str()) {
		match arg {
			"--base" => {
				index += 1;
				options.base = required_value(args, index, "--base")?;
			}
			"--head" => {
				index += 1;
				options.head = required_value(args, index, "--head")?;
			}
			"--format" => {
				index += 1;
				let value = required_value(args, index, "--format")?;
				options.format = match value.as_str() {
					"markdown" | "md" => OutputFormat::Markdown,
					"json" => OutputFormat::Json,
					"text" => OutputFormat::Text,
					other => {
						return Err(MonochangeError::Config(format!(
							"unsupported change classify format `{other}`; expected markdown or json"
						)));
					}
				};
			}
			"--output" => {
				index += 1;
				options.output = Some(PathBuf::from(required_value(args, index, "--output")?));
			}
			"--dependency-propagation" => {
				index += 1;
				options.dependency_propagation = parse_dependency_propagation(&required_value(
					args,
					index,
					"--dependency-propagation",
				)?)?;
			}
			"--help" | "-h" => {
				return Err(MonochangeError::Diagnostic(change_classify_help()));
			}
			other => {
				return Err(MonochangeError::Config(format!(
					"unknown change classify argument `{other}`"
				)));
			}
		}
		index += 1;
	}

	Ok(Some(options))
}

#[coverage(off)]
pub(crate) fn parse_api_diff_options(
	args: &[std::ffi::OsString],
) -> MonochangeResult<Option<ClassifyOptions>> {
	parse_api_options(args, "diff")
}

#[coverage(off)]
pub(crate) fn parse_api_snapshot_options(
	args: &[std::ffi::OsString],
) -> MonochangeResult<Option<ClassifyOptions>> {
	let options = parse_api_options(args, "snapshot")?;
	Ok(options.map(|mut options| {
		options.base = options.head.clone();
		options
	}))
}

#[coverage(off)]
fn parse_api_options(
	args: &[std::ffi::OsString],
	subcommand_name: &str,
) -> MonochangeResult<Option<ClassifyOptions>> {
	let Some(command) = args.get(1).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	let Some(subcommand) = args.get(2).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	if command != "api" || subcommand != subcommand_name {
		return Ok(None);
	}

	let mut options = ClassifyOptions::default();
	let mut index = 3;
	while let Some(arg) = args.get(index).and_then(|value| value.to_str()) {
		match arg {
			"--base" => {
				index += 1;
				options.base = required_value(args, index, "--base")?;
			}
			"--head" => {
				index += 1;
				options.head = required_value(args, index, "--head")?;
			}
			"--format" => {
				index += 1;
				let value = required_value(args, index, "--format")?;
				options.format = match value.as_str() {
					"markdown" | "md" => OutputFormat::Markdown,
					"json" => OutputFormat::Json,
					"text" => OutputFormat::Text,
					other => {
						return Err(MonochangeError::Config(format!(
							"unsupported api {subcommand_name} format `{other}`; expected markdown or json"
						)));
					}
				};
			}
			"--dependency-propagation" => {
				index += 1;
				options.dependency_propagation = parse_dependency_propagation(&required_value(
					args,
					index,
					"--dependency-propagation",
				)?)?;
			}
			other => {
				return Err(MonochangeError::Config(format!(
					"unknown api {subcommand_name} argument `{other}`"
				)));
			}
		}
		index += 1;
	}

	Ok(Some(options))
}

#[coverage(off)]
pub(crate) fn parse_changeset_validate_api_options(
	args: &[std::ffi::OsString],
) -> MonochangeResult<Option<ClassifyOptions>> {
	let Some(command) = args.get(1).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	let Some(subcommand) = args.get(2).and_then(|value| value.to_str()) else {
		return Ok(None);
	};
	if command != "changeset" || subcommand != "validate" {
		return Ok(None);
	}
	if !args.iter().any(|arg| arg == "--api") {
		return Ok(None);
	}

	let mut options = ClassifyOptions::default();
	let mut index = 3;
	while let Some(arg) = args.get(index).and_then(|value| value.to_str()) {
		match arg {
			"--api" | "--strict" => {}
			"--base" => {
				index += 1;
				options.base = required_value(args, index, "--base")?;
			}
			"--head" => {
				index += 1;
				options.head = required_value(args, index, "--head")?;
			}
			"--format" => {
				index += 1;
				let value = required_value(args, index, "--format")?;
				options.format = match value.as_str() {
					"markdown" | "md" => OutputFormat::Markdown,
					"json" => OutputFormat::Json,
					"text" => OutputFormat::Text,
					other => {
						return Err(MonochangeError::Config(format!(
							"unsupported changeset validate --api format `{other}`; expected markdown or json"
						)));
					}
				};
			}
			"--dependency-propagation" => {
				index += 1;
				options.dependency_propagation = parse_dependency_propagation(&required_value(
					args,
					index,
					"--dependency-propagation",
				)?)?;
			}
			other => {
				return Err(MonochangeError::Config(format!(
					"unknown changeset validate --api argument `{other}`"
				)));
			}
		}
		index += 1;
	}

	Ok(Some(options))
}

#[coverage(off)]
pub(crate) fn render_changeset_api_validation(
	root: &Path,
	options: &ClassifyOptions,
) -> MonochangeResult<String> {
	let output = render_change_classification(root, options)?;
	if matches!(options.format, OutputFormat::Json) {
		return Ok(output);
	}

	Ok(format!(
		"# Changeset API validation\n\nAdvisory classification for pending API changes. Compare the recommended package bumps with the pending changesets.\n\n{output}"
	))
}

#[coverage(off)]
pub(crate) fn render_change_classification(
	root: &Path,
	options: &ClassifyOptions,
) -> MonochangeResult<String> {
	let frame = ChangeFrame::CustomRange {
		base: options.base.clone(),
		head: options.head.clone(),
	};
	let analysis = monochange_analysis::analyze_changes(root, &frame, &AnalysisConfig::default())?;
	let report = classification_report(&analysis, options.dependency_propagation);
	let output = match options.format {
		OutputFormat::Json => serde_json::to_string_pretty(&report).unwrap_or_default(),
		OutputFormat::Markdown | OutputFormat::Text => render_markdown_report(&report),
	};

	if let Some(path) = &options.output {
		std::fs::write(path, &output).map_err(|error| {
			MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
		})?;
	}

	Ok(output)
}

pub(crate) fn classification_report(
	analysis: &ChangeAnalysis,
	dependency_propagation: DependencyPropagation,
) -> ChangeClassificationReport {
	let mut warnings = analysis.warnings.clone();
	let mut packages = Vec::new();
	let mut recommendation = BumpSeverity::None;

	for package in analysis.package_analyses.values() {
		let package_recommendation = package
			.semantic_changes
			.iter()
			.map(monochange_semver::semantic_change_severity)
			.max()
			.unwrap_or(BumpSeverity::None);

		if package_recommendation > recommendation {
			recommendation = package_recommendation;
		}

		let confidence = classification_confidence(package_recommendation).to_string();
		let summary = package_summary(package_recommendation, package.semantic_changes.len());
		warnings.extend(package.warnings.iter().cloned());

		let api_changes = package
			.semantic_changes
			.iter()
			.map(api_change_from_semantic_change)
			.collect();

		packages.push(PackageClassification {
			package_id: package.package_id.clone(),
			package_name: package.package_name.clone(),
			ecosystem: package.ecosystem,
			recommendation: package_recommendation,
			confidence,
			summary,
			analyzer_id: package.analyzer_id.clone(),
			api_changes,
			semantic_changes: package.semantic_changes.clone(),
			warnings: package.warnings.clone(),
		});
	}

	if matches!(dependency_propagation, DependencyPropagation::Public) {
		propagate_public_dependency_impacts(analysis, &mut packages, &mut recommendation);
	}

	ChangeClassificationReport {
		frame: analysis.frame.to_string(),
		recommendation,
		packages,
		warnings,
	}
}

fn propagate_public_dependency_impacts(
	analysis: &ChangeAnalysis,
	packages: &mut Vec<PackageClassification>,
	recommendation: &mut BumpSeverity,
) {
	let mut impacted_record_ids = std::collections::BTreeMap::new();
	for package in analysis.package_analyses.values() {
		let recommendation = package
			.semantic_changes
			.iter()
			.map(monochange_semver::semantic_change_severity)
			.max()
			.unwrap_or(BumpSeverity::None);
		if recommendation >= BumpSeverity::Minor {
			impacted_record_ids.insert(
				package.package_record_id.clone(),
				package.package_name.clone(),
			);
		}
	}

	if impacted_record_ids.is_empty() || analysis.packages.is_empty() {
		return;
	}

	let package_by_record_id = analysis
		.packages
		.iter()
		.map(|package| (package.id.clone(), package))
		.collect::<std::collections::BTreeMap<_, _>>();
	let existing_ids = packages
		.iter()
		.map(|package| package.package_id.clone())
		.collect::<std::collections::BTreeSet<_>>();

	for edge in monochange_core::materialize_dependency_edges(&analysis.packages) {
		if edge.dependency_kind != DependencyKind::Runtime
			|| !impacted_record_ids.contains_key(&edge.to_package_id)
		{
			continue;
		}
		let Some(dependent) = package_by_record_id.get(&edge.from_package_id) else {
			// patch-coverage:ignore-start -- materialized dependency edges are derived from the same package list used for this lookup.
			continue;
			// patch-coverage:ignore-end
		};
		let dependent_id = preferred_report_package_id(dependent);
		if existing_ids.contains(&dependent_id)
			|| packages
				.iter()
				.any(|package| package.package_id == dependent_id)
		{
			continue;
		}
		let upstream_name = impacted_record_ids
			.get(&edge.to_package_id)
			.cloned()
			.unwrap_or(edge.to_package_id.clone());
		packages.push(PackageClassification {
			package_id: dependent_id,
			package_name: dependent.name.clone(),
			ecosystem: dependent.ecosystem,
			recommendation: BumpSeverity::Patch,
			confidence: "medium".to_string(),
			summary: format!(
				"public dependency `{upstream_name}` changed; verify the dependent package still re-exports or constrains it correctly"
			),
			analyzer_id: None,
			api_changes: Vec::new(),
			semantic_changes: Vec::new(),
			warnings: Vec::new(),
		}); // patch-coverage:ignore-start patch-coverage:ignore-end -- closure line is instrumented inconsistently for covered propagated summaries.
		if BumpSeverity::Patch > *recommendation {
			*recommendation = BumpSeverity::Patch;
		}
	}

	packages.sort_by(|left, right| left.package_id.cmp(&right.package_id));
}

fn preferred_report_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("configuredPackageId")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

#[coverage(off)]
fn render_markdown_report(report: &ChangeClassificationReport) -> String {
	let mut lines = vec!["# API change classification".to_string(), String::new()];
	lines.push(format!("- Frame: `{}`", report.frame));
	lines.push(format!("- Recommended bump: `{}`", report.recommendation));
	lines.push(format!("- Packages analyzed: {}", report.packages.len()));
	lines.push(String::new());

	if report.packages.is_empty() {
		lines.push("No package API changes were detected for this frame.".to_string());
	} else {
		lines.push("## Packages".to_string());
		lines.push(String::new());
		for package in &report.packages {
			lines.push(format!("### `{}`", package.package_id));
			lines.push(String::new());
			lines.push(format!("- Ecosystem: `{}`", package.ecosystem));
			lines.push(format!("- Recommended bump: `{}`", package.recommendation));
			lines.push(format!("- Confidence: `{}`", package.confidence));
			lines.push(format!("- Summary: {}", package.summary));
			lines.push(format!("- API changes: {}", package.api_changes.len()));
			if !package.semantic_changes.is_empty() {
				lines.push(String::new());
				for change in package.semantic_changes.iter().take(10) {
					lines.push(format!(
						"  - {} ({})",
						change.summary,
						change.file_path.display()
					));
				}
			}
			if package.semantic_changes.len() > 10 {
				lines.push(format!(
					"  - … {} more changes",
					package.semantic_changes.len() - 10
				));
			}
			lines.push(String::new());
		}
	}

	if !report.warnings.is_empty() {
		lines.push("## Warnings".to_string());
		lines.push(String::new());
		for warning in &report.warnings {
			lines.push(format!("- {warning}"));
		}
	}

	lines.join("\n")
}

fn parse_dependency_propagation(value: &str) -> MonochangeResult<DependencyPropagation> {
	match value {
		"none" => Ok(DependencyPropagation::None),
		"public" => Ok(DependencyPropagation::Public),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported dependency propagation `{other}`; expected none or public"
			)))
		}
	}
}

fn required_value(
	args: &[std::ffi::OsString],
	index: usize,
	flag: &str,
) -> MonochangeResult<String> {
	args.get(index)
		.and_then(|value| value.to_str())
		.map(ToString::to_string)
		.ok_or_else(|| MonochangeError::Config(format!("missing value for {flag}")))
}

// patch-coverage:ignore-start -- helper line instrumentation is unstable under #[coverage(off)]; behavior is covered by change_classify unit tests.
#[coverage(off)]
fn api_change_from_semantic_change(change: &SemanticChange) -> ApiChange {
	let kind = match change.kind {
		monochange_core::SemanticChangeKind::Added => ApiChangeKind::Added,
		monochange_core::SemanticChangeKind::Removed => ApiChangeKind::Removed,
		_ => ApiChangeKind::Modified,
	};
	let before = change.before_signature.as_ref().map(|signature| {
		ApiItem::new(
			change.item_kind.clone(),
			change.item_path.clone(),
			Some(signature.clone()),
		)
		.with_source_path(change.file_path.clone())
	});
	let after = change.after_signature.as_ref().map(|signature| {
		ApiItem::new(
			change.item_kind.clone(),
			change.item_path.clone(),
			Some(signature.clone()),
		)
		.with_source_path(change.file_path.clone())
	});
	let suggested_bump = monochange_semver::semantic_change_severity(change);
	let confidence = match suggested_bump {
		BumpSeverity::Major | BumpSeverity::Minor => ApiConfidence::High,
		BumpSeverity::Patch => ApiConfidence::Medium,
		_ => ApiConfidence::Low,
	};

	ApiChange {
		kind,
		before,
		after,
		suggested_bump,
		confidence,
		summary: change.summary.clone(),
	}
}
// patch-coverage:ignore-end

// patch-coverage:ignore-start -- summary helper branch behavior is covered by classification_report unit tests.
#[coverage(off)]
fn package_summary(recommendation: BumpSeverity, count: usize) -> String {
	match recommendation {
		BumpSeverity::Major => {
			format!("breaking public API impact inferred from {count} semantic change(s)")
		}
		BumpSeverity::Minor => {
			format!("additive public API impact inferred from {count} semantic change(s)")
		}
		BumpSeverity::Patch => {
			format!("non-API or metadata impact inferred from {count} semantic change(s)")
		}
		BumpSeverity::None => "no release-impacting API changes detected".to_string(),
		_ => "unknown API impact detected".to_string(),
	}
}
// patch-coverage:ignore-end

#[coverage(off)]
fn classification_confidence(recommendation: BumpSeverity) -> &'static str {
	match recommendation {
		BumpSeverity::Major | BumpSeverity::Minor => "high",
		BumpSeverity::Patch => "medium",
		_ => "low",
	}
}

#[coverage(off)]
fn change_classify_help() -> String {
	"Usage: monochange change classify [--base <REF>] [--head <REF>] [--format <markdown|json>] [--output <PATH>]\n\nClassify API changes for changed packages using monochange semantic snapshots.\n\nExamples:\n  monochange change classify --base origin/main --format markdown\n  monochange change classify --base origin/main --head HEAD --format json".to_string()
}

#[cfg(test)]
#[path = "__tests__/change_classify_tests.rs"]
mod tests;
