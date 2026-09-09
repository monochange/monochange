use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use monochange_analysis::AnalysisConfig;
use monochange_analysis::AnalysisSession;
use monochange_analysis::ChangeAnalysis;
use monochange_analysis::ChangeFrame;
use monochange_core::BumpSeverity;
use monochange_core::DependencyKind;
use monochange_core::DetectionLevel;
use monochange_core::EffectiveReleaseIdentity;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::ReleaseOwnerKind;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use monochange_core::VersionFormat;
use serde::Deserialize;
use serde::Serialize;

use crate::OutputFormat;

const DEFAULT_HEAD_REF: &str = "HEAD";
const CHANGE_CLASSIFICATION_SCHEMA_VERSION: u16 = 1;
const ANALYZER_VERSION: &str = "1";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ClassifyOptions {
	pub(crate) base: Option<String>,
	pub(crate) head: String,
	pub(crate) release: Option<String>,
	pub(crate) packages: Vec<String>,
	pub(crate) detection_level: DetectionLevel,
	pub(crate) include_unchanged: bool,
	pub(crate) strict: bool,
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
			base: None,
			head: DEFAULT_HEAD_REF.to_string(),
			release: None,
			packages: Vec::new(),
			detection_level: DetectionLevel::Signature,
			include_unchanged: false,
			strict: false,
			format: OutputFormat::Text,
			output: None,
			dependency_propagation: DependencyPropagation::None,
		}
	}
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ComparisonKind {
	PullRequest,
	Release,
	ReleaseToDefault,
	SourceDelta,
	WorkingTree,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ComparisonStatus {
	Analyzed,
	Unavailable,
	Conflicted,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedComparison {
	pub(crate) kind: ComparisonKind,
	pub(crate) base: Option<String>,
	pub(crate) head: String,
	pub(crate) status: ComparisonStatus,
	pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompatibilityImpact {
	Unknown,
	Compatible,
	Additive,
	Breaking,
}

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ClassificationConfidence {
	Low,
	Medium,
	High,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AnalysisCompleteness {
	Complete,
	Partial,
	Unsupported,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FindingAnalyzer {
	pub(crate) id: String,
	pub(crate) version: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FindingCoverage {
	pub(crate) detection_level: DetectionLevel,
	pub(crate) completeness: AnalysisCompleteness,
	pub(crate) note: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClassificationFinding {
	pub(crate) id: String,
	pub(crate) rule_id: String,
	pub(crate) surface: String,
	pub(crate) change: String,
	pub(crate) impact: CompatibilityImpact,
	pub(crate) bump: BumpSeverity,
	pub(crate) confidence: ClassificationConfidence,
	pub(crate) analyzer: FindingAnalyzer,
	pub(crate) coverage: FindingCoverage,
	pub(crate) before: Option<String>,
	pub(crate) after: Option<String>,
	pub(crate) location: PathBuf,
	pub(crate) comparisons: BTreeSet<ComparisonKind>,
	pub(crate) summary: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeRecommendation {
	pub(crate) compatibility_impact: CompatibilityImpact,
	pub(crate) proposed_changeset_bump: BumpSeverity,
	pub(crate) enforceable_minimum: BumpSeverity,
	pub(crate) release_floor: BumpSeverity,
	pub(crate) confidence: ClassificationConfidence,
	pub(crate) completeness: AnalysisCompleteness,
	pub(crate) review_required: bool,
	pub(crate) finding_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ChangesetAction {
	Create,
	Update,
	Keep,
	Review,
	NoChangeset,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExistingChangeset {
	pub(crate) path: PathBuf,
	pub(crate) bump: Option<BumpSeverity>,
	pub(crate) change_type: Option<String>,
}

type ExistingChangesetsByPackage = BTreeMap<String, Vec<ExistingChangeset>>;
type ExistingChangesetInventory = (ExistingChangesetsByPackage, Vec<String>);

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseOwner {
	pub(crate) kind: String,
	pub(crate) id: String,
	pub(crate) latest_release: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangeClassificationReport {
	pub(crate) schema_version: u16,
	pub(crate) default_branch: String,
	pub(crate) candidate: String,
	pub(crate) comparisons: Vec<ResolvedComparison>,
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
	pub(crate) release_owner: Option<ReleaseOwner>,
	pub(crate) comparisons: Vec<ResolvedComparison>,
	pub(crate) recommendation: BumpSeverity,
	pub(crate) decision: ChangeRecommendation,
	pub(crate) summary: String,
	pub(crate) findings: Vec<ClassificationFinding>,
	pub(crate) existing_changesets: Vec<ExistingChangeset>,
	pub(crate) action: ChangesetAction,
	pub(crate) warnings: Vec<String>,
}

pub(crate) fn classify_options_from_matches(
	matches: &clap::ArgMatches,
) -> MonochangeResult<ClassifyOptions> {
	let format = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	let dependency_propagation = matches
		.get_one::<String>("dependency-propagation")
		.map_or("none", String::as_str);

	Ok(ClassifyOptions {
		base: matches.get_one::<String>("base").cloned(),
		head: matches
			.get_one::<String>("head")
			.cloned()
			.unwrap_or_else(|| DEFAULT_HEAD_REF.to_string()),
		release: matches.get_one::<String>("release").cloned(),
		packages: matches
			.get_many::<String>("package")
			.into_iter()
			.flatten()
			.cloned()
			.collect(),
		// patch-coverage:ignore-start -- clap restricts this value before extraction; parser success and direct error paths are covered separately, while llvm-cov attributes the propagated `?` to this call site.
		detection_level: parse_detection_level(
			matches
				.get_one::<String>("detection-level")
				.map_or("signature", String::as_str),
		)?,
		// patch-coverage:ignore-end
		include_unchanged: matches.get_flag("include-unchanged"),
		strict: matches
			.try_get_one::<bool>("strict")
			.ok()
			.flatten()
			.copied()
			.unwrap_or(false),
		format: match format {
			"markdown" | "md" => OutputFormat::Markdown,
			"json" => OutputFormat::Json,
			"json-min" => OutputFormat::JsonMin,
			"text" => OutputFormat::Text,
			// patch-coverage:ignore-start -- Clap constrains this value before option extraction; direct parser tests cover accepted values.
			other => {
				return Err(MonochangeError::Config(format!(
					"unsupported classification format `{other}`; expected markdown, json, json-min, or text"
				)));
			} // patch-coverage:ignore-end
		},
		output: matches.get_one::<String>("output").map(PathBuf::from),
		dependency_propagation: parse_dependency_propagation(dependency_propagation)?,
	})
}

#[coverage(off)]
pub(crate) fn render_changeset_api_validation(
	root: &Path,
	options: &ClassifyOptions,
) -> MonochangeResult<String> {
	let report = build_change_classification_report(root, options)?;
	let mismatches = changeset_validation_mismatches(&report, options.strict);
	if !mismatches.is_empty() {
		return Err(MonochangeError::Config(format!(
			"changeset severity does not satisfy classification:\n- {}",
			mismatches.join("\n- ")
		)));
	}
	let output = match options.format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			options
				.format
				.render_json_value(&report, "changeset API validation")?
		}
		OutputFormat::Markdown => {
			format!(
				"# Changeset API validation\n\nPending changesets satisfy the enforceable minimum. Medium- and low-confidence findings remain advisory unless `--strict` is set.\n\n{}",
				render_markdown_report(&report)
			)
		}
		OutputFormat::Text => {
			format!(
				"Changeset API validation\n\nPending changesets satisfy the enforceable minimum. Medium- and low-confidence findings remain advisory unless --strict is set.\n\n{}",
				render_text_report(&report)
			)
		}
	};
	if let Some(path) = &options.output {
		std::fs::write(path, &output).map_err(|error| {
			MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
		})?;
	}

	Ok(output)
}

fn changeset_validation_mismatches(
	report: &ChangeClassificationReport,
	strict: bool,
) -> Vec<String> {
	report
		.packages
		.iter()
		.filter_map(|package| {
			let required = if strict {
				package.decision.proposed_changeset_bump
			} else {
				package.decision.enforceable_minimum
			};
			if required == BumpSeverity::None {
				return None;
			}

			let declared = package
				.existing_changesets
				.iter()
				.filter_map(|changeset| changeset.bump)
				.max()
				.unwrap_or(BumpSeverity::None);
			(declared < required).then(|| {
				format!(
					"package `{}` declares `{declared}` but requires at least `{required}`{}",
					package.package_id,
					if strict { " in strict mode" } else { "" }
				)
			})
		})
		.collect()
}

#[coverage(off)]
pub(crate) fn render_change_classification(
	root: &Path,
	options: &ClassifyOptions,
) -> MonochangeResult<String> {
	let report = build_change_classification_report(root, options)?;
	let output = match options.format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			options
				.format
				.render_json_value(&report, "change classification")?
		}
		OutputFormat::Markdown => render_markdown_report(&report),
		OutputFormat::Text => render_text_report(&report),
	};

	if let Some(path) = &options.output {
		std::fs::write(path, &output).map_err(|error| {
			MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
		})?;
	}

	Ok(output)
}

struct CandidateResolution {
	reference: String,
	display: String,
	source_reference: String,
	source_display: String,
	includes_working_tree: bool,
	status: ComparisonStatus,
	note: Option<String>,
}

struct PackageEvidence<'a> {
	kind: ComparisonKind,
	analysis: &'a ChangeAnalysis,
}

#[derive(Debug, Clone)]
struct PublicDependencyImpact {
	dependent: PackageRecord,
	upstream_name: String,
}

pub(crate) fn build_change_classification_report(
	root: &Path,
	options: &ClassifyOptions,
) -> MonochangeResult<ChangeClassificationReport> {
	let configuration = monochange_config::load_workspace_configuration(root)?;
	let default_branch = options
		.base
		.clone()
		.map_or_else(|| resolve_default_branch_ref(root), Ok)?;
	let candidate = resolve_candidate(root, &default_branch, &options.head)?;
	let analysis_config = AnalysisConfig {
		detection_level: options.detection_level,
		..AnalysisConfig::default()
	};
	let analysis_session = AnalysisSession::new(root, analysis_config)?;
	let pull_request = analyze_range(&analysis_session, &default_branch, &candidate.reference)?;
	let source_base = resolve_merge_base(root, &default_branch, &candidate.source_reference)
		.unwrap_or_else(|| default_branch.clone());
	let source_delta = analyze_range(&analysis_session, &source_base, &candidate.source_reference)?;
	let working_tree = working_tree_analysis(root, &options.head, &analysis_session)?;
	let packages = pull_request.packages.clone();
	let mut warnings = pull_request
		.warnings
		.iter()
		.chain(source_delta.warnings.iter())
		.cloned()
		.collect::<Vec<_>>();

	if let Some(working_tree) = &working_tree {
		warnings.extend(working_tree.warnings.iter().cloned());
	}
	if let Some(note) = &candidate.note {
		warnings.push(note.clone());
	}

	let mut selected_ids = selected_package_ids(root, &packages, &pull_request, options)?;
	if matches!(
		options.dependency_propagation,
		DependencyPropagation::Public
	) {
		selected_ids.extend(
			public_dependency_impacts(&pull_request)
				.into_iter()
				.map(|impact| preferred_report_package_id(&impact.dependent)),
		);
	}
	// patch-coverage:ignore-start -- successful and invalid changeset inventory paths are covered end to end; llvm-cov attributes the propagated `?` line inconsistently.
	let (existing_changesets, changeset_warnings) = existing_changesets_by_package(
		root,
		&configuration,
		&packages,
		&source_delta,
		working_tree.as_ref(),
	)?;
	// patch-coverage:ignore-end
	warnings.extend(changeset_warnings);
	if options.packages.is_empty() {
		selected_ids.extend(existing_changesets.keys().cloned());
	}

	let mut release_analyses = BTreeMap::<String, (ChangeAnalysis, ChangeAnalysis)>::new();
	let mut package_reports = Vec::new();
	for package in &packages {
		let package_id = preferred_report_package_id(package);
		if !selected_ids.remove(&package_id) {
			continue;
		}

		let release_identity = pull_request
			.package_analyses
			.get(&package_id)
			.and_then(|analysis| analysis.release_identity.clone())
			.or_else(|| configuration.effective_release_identity(&package_id));
		let latest_release = match &options.release {
			Some(release) => Some(release.clone()),
			None => {
				// patch-coverage:ignore-start -- automatic tag discovery is covered directly; llvm-cov attributes the propagated `?` to this call site.
				latest_release_tag(
					root,
					&default_branch,
					release_identity.as_ref(),
					package.ecosystem.as_str(),
				)?
				// patch-coverage:ignore-end
			}
		};
		let mut comparisons = vec![
			ResolvedComparison {
				kind: ComparisonKind::PullRequest,
				base: Some(default_branch.clone()),
				head: candidate.display.clone(),
				status: candidate.status,
				note: candidate.note.clone(),
			},
			ResolvedComparison {
				kind: ComparisonKind::SourceDelta,
				base: Some(source_base.clone()),
				head: candidate.source_display.clone(),
				status: ComparisonStatus::Analyzed,
				note: candidate.includes_working_tree.then(|| {
					"the source comparison includes staged, unstaged, deleted, and untracked files"
						.to_string()
				}),
			},
		];
		let mut evidence = vec![
			PackageEvidence {
				kind: ComparisonKind::PullRequest,
				analysis: &pull_request,
			},
			PackageEvidence {
				kind: ComparisonKind::SourceDelta,
				analysis: &source_delta,
			},
		];

		if let Some(working_tree) = &working_tree {
			comparisons.push(ResolvedComparison {
				kind: ComparisonKind::WorkingTree,
				base: Some(options.head.clone()),
				head: "working tree".to_string(),
				status: ComparisonStatus::Analyzed,
				note: Some(
					"staged, unstaged, deleted, and untracked files are included".to_string(),
				),
			});
			evidence.push(PackageEvidence {
				kind: ComparisonKind::WorkingTree,
				analysis: working_tree,
			});
		}

		if let Some(release) = &latest_release {
			if !release_analyses.contains_key(release) {
				let release_to_candidate =
					analyze_range(&analysis_session, release, &candidate.reference)?;
				let release_to_default =
					analyze_range(&analysis_session, release, &default_branch)?;
				release_analyses
					.insert(release.clone(), (release_to_candidate, release_to_default));
			}

			let (release_to_candidate, release_to_default) = release_analyses
				.get(release)
				.expect("release analysis was inserted before lookup");
			comparisons.extend([
				ResolvedComparison {
					kind: ComparisonKind::Release,
					base: Some(release.clone()),
					head: candidate.display.clone(),
					status: candidate.status,
					note: candidate.note.clone(),
				},
				ResolvedComparison {
					kind: ComparisonKind::ReleaseToDefault,
					base: Some(release.clone()),
					head: default_branch.clone(),
					status: ComparisonStatus::Analyzed,
					note: None,
				},
			]);
			evidence.extend([
				PackageEvidence {
					kind: ComparisonKind::Release,
					analysis: release_to_candidate,
				},
				PackageEvidence {
					kind: ComparisonKind::ReleaseToDefault,
					analysis: release_to_default,
				},
			]);
		} else {
			comparisons.extend([
				ResolvedComparison {
					kind: ComparisonKind::Release,
					base: None,
					head: candidate.display.clone(),
					status: ComparisonStatus::Unavailable,
					note: Some("no reachable release tag matched this package".to_string()),
				},
				ResolvedComparison {
					kind: ComparisonKind::ReleaseToDefault,
					base: None,
					head: default_branch.clone(),
					status: ComparisonStatus::Unavailable,
					note: Some("no reachable release tag matched this package".to_string()),
				},
			]);
		}

		let changed_files = pull_request_changed_files(&package_id, &pull_request);
		let mut findings = collect_findings(&package_id, &evidence, options.detection_level);
		ensure_unclassified_finding(
			&package_id,
			package.ecosystem,
			options.detection_level,
			&changed_files,
			&mut findings,
		);
		let package_changesets = existing_changesets
			.get(&package_id)
			.cloned()
			.unwrap_or_default();
		let mut decision = build_recommendation(
			&findings,
			!changed_files.is_empty(),
			latest_release.is_some(),
		);
		if changed_files.is_empty() && !package_changesets.is_empty() {
			decision.compatibility_impact = CompatibilityImpact::Unknown;
			decision.completeness = AnalysisCompleteness::Unsupported;
			decision.review_required = true;
		}
		let action = changeset_action(&decision, &package_changesets);
		let summary = recommendation_summary(&decision, &findings);
		let package_warnings = package_warnings(&package_id, &evidence);

		package_reports.push(PackageClassification {
			package_id,
			package_name: package.name.clone(),
			ecosystem: package.ecosystem,
			release_owner: release_owner(release_identity.as_ref(), latest_release),
			comparisons,
			recommendation: decision.proposed_changeset_bump,
			decision,
			summary,
			findings,
			existing_changesets: package_changesets,
			action,
			warnings: package_warnings,
		});
	}

	if !selected_ids.is_empty() {
		// patch-coverage:ignore-start -- explicit references are resolved before insertion, and propagated ids come from the same discovered package set.
		return Err(MonochangeError::Config(format!(
			"package selection did not match a discovered package: {}",
			selected_ids.into_iter().collect::<Vec<_>>().join(", ")
		)));
		// patch-coverage:ignore-end
	}

	if matches!(
		options.dependency_propagation,
		DependencyPropagation::Public
	) {
		let mut recommendation = package_reports
			.iter()
			.map(|package| package.recommendation)
			.max()
			.unwrap_or(BumpSeverity::None);
		propagate_public_dependency_impacts(
			&pull_request,
			&mut package_reports,
			&mut recommendation,
		);
	}

	package_reports.sort_by(|left, right| left.package_id.cmp(&right.package_id));
	let recommendation = package_reports
		.iter()
		.map(|package| package.recommendation)
		.max()
		.unwrap_or(BumpSeverity::None);
	let mut comparisons = vec![
		ResolvedComparison {
			kind: ComparisonKind::PullRequest,
			base: Some(default_branch.clone()),
			head: candidate.display.clone(),
			status: candidate.status,
			note: candidate.note.clone(),
		},
		ResolvedComparison {
			kind: ComparisonKind::SourceDelta,
			base: Some(source_base),
			head: candidate.source_display.clone(),
			status: ComparisonStatus::Analyzed,
			note: candidate.includes_working_tree.then(|| {
				"the source comparison includes staged, unstaged, deleted, and untracked files"
					.to_string()
			}),
		},
	];
	if working_tree.is_some() {
		comparisons.push(ResolvedComparison {
			kind: ComparisonKind::WorkingTree,
			base: Some(options.head.clone()),
			head: "working tree".to_string(),
			status: ComparisonStatus::Analyzed,
			note: Some(
				"local changes are reported separately and are also materialized into the final candidate"
					.to_string(),
			),
		});
	}
	warnings.sort();
	warnings.dedup();

	Ok(ChangeClassificationReport {
		schema_version: CHANGE_CLASSIFICATION_SCHEMA_VERSION,
		default_branch,
		candidate: candidate.display,
		comparisons,
		recommendation,
		packages: package_reports,
		warnings,
	})
}

pub(crate) fn classification_report(
	analysis: &ChangeAnalysis,
	dependency_propagation: DependencyPropagation,
) -> ChangeClassificationReport {
	let mut warnings = analysis.warnings.clone();
	let mut packages = Vec::new();
	let mut recommendation = BumpSeverity::None;

	for package in analysis.package_analyses.values() {
		let evidence = [PackageEvidence {
			kind: ComparisonKind::PullRequest,
			analysis,
		}];
		let mut findings =
			collect_findings(&package.package_id, &evidence, analysis.detection_level);
		ensure_unclassified_finding(
			&package.package_id,
			package.ecosystem,
			analysis.detection_level,
			&package.changed_files,
			&mut findings,
		);
		let decision = build_recommendation(&findings, !package.changed_files.is_empty(), false);
		let package_recommendation = decision.proposed_changeset_bump;

		if package_recommendation > recommendation {
			recommendation = package_recommendation;
		}

		let summary = recommendation_summary(&decision, &findings);
		warnings.extend(package.warnings.iter().cloned());

		packages.push(PackageClassification {
			package_id: package.package_id.clone(),
			package_name: package.package_name.clone(),
			ecosystem: package.ecosystem,
			release_owner: None,
			comparisons: Vec::new(),
			recommendation: package_recommendation,
			decision,
			summary,
			findings,
			existing_changesets: Vec::new(),
			action: ChangesetAction::Create,
			warnings: package.warnings.clone(),
		});
	}

	if matches!(dependency_propagation, DependencyPropagation::Public) {
		propagate_public_dependency_impacts(analysis, &mut packages, &mut recommendation);
	}

	ChangeClassificationReport {
		schema_version: CHANGE_CLASSIFICATION_SCHEMA_VERSION,
		default_branch: analysis.frame.base_revision().unwrap_or("HEAD").to_string(),
		candidate: analysis
			.frame
			.head_revision()
			.unwrap_or("working tree")
			.to_string(),
		comparisons: vec![ResolvedComparison {
			kind: ComparisonKind::PullRequest,
			base: analysis.frame.base_revision().map(ToString::to_string),
			head: analysis
				.frame
				.head_revision()
				.unwrap_or("working tree")
				.to_string(),
			status: ComparisonStatus::Analyzed,
			note: None,
		}],
		recommendation,
		packages,
		warnings,
	}
}

fn analyze_range(
	session: &AnalysisSession,
	base: &str,
	head: &str,
) -> MonochangeResult<ChangeAnalysis> {
	session.analyze(&ChangeFrame::CustomRange {
		base: base.to_string(),
		head: head.to_string(),
	})
}

fn resolve_default_branch_ref(root: &Path) -> MonochangeResult<String> {
	let symbolic = run_git(root, &["rev-parse", "--abbrev-ref", "origin/HEAD"])
		.ok()
		.map(|value| value.trim().to_string())
		.filter(|value| value != "origin/HEAD" && !value.is_empty());

	if let Some(symbolic) = symbolic {
		return Ok(symbolic);
	}

	for branch in ["origin/main", "main", "origin/master", "master"] {
		if git_revision_exists(root, branch) {
			return Ok(branch.to_string());
		}
	}

	Err(MonochangeError::Discovery(
		"could not determine the default branch ref".to_string(),
	))
}

fn resolve_candidate(root: &Path, base: &str, head: &str) -> MonochangeResult<CandidateResolution> {
	let (source_reference, source_display, includes_working_tree) = if head == DEFAULT_HEAD_REF
		&& !ChangeFrame::WorkingDirectory
			.changed_files(root)?
			.is_empty()
	{
		let reference = materialize_working_tree_commit(root, head)?;
		let short_reference = short_revision(&reference);
		(reference, format!("working-tree:{short_reference}"), true)
	} else {
		(head.to_string(), head.to_string(), false)
	};
	let merge_tree = run_git(
		root,
		&["merge-tree", "--write-tree", base, &source_reference],
	);
	match merge_tree {
		Ok(output) => {
			let tree = output
				.lines()
				.next()
				.unwrap_or(&source_reference)
				.trim()
				.to_string();
			let short_tree = short_revision(&tree);
			Ok(CandidateResolution {
				reference: tree,
				display: format!("merge-tree:{short_tree}"),
				source_reference,
				source_display,
				includes_working_tree,
				status: ComparisonStatus::Analyzed,
				note: includes_working_tree.then(|| {
					"the merge candidate includes staged, unstaged, deleted, and untracked files"
						.to_string()
				}),
			})
		}
		Err(error) => {
			Ok(CandidateResolution {
				reference: source_reference.clone(),
				display: source_display.clone(),
				source_reference,
				source_display,
				includes_working_tree,
				status: ComparisonStatus::Conflicted,
				note: Some(format!(
					"could not build a merge candidate for `{base}` and `{head}`; the pull request comparison falls back to the source candidate: {error}"
				)),
			})
		}
	}
}

fn materialize_working_tree_commit(root: &Path, head: &str) -> MonochangeResult<String> {
	// patch-coverage:ignore-start -- temporary-directory creation failure depends on host filesystem exhaustion; materialization behavior is covered by net-candidate integration tests.
	let temporary = tempfile::tempdir().map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create a temporary index for change classification: {error}"
		))
	})?;
	// patch-coverage:ignore-end
	let index_path = temporary.path().join("index");
	run_git_with_index(root, &index_path, &["read-tree", head])?;
	run_git_with_index(root, &index_path, &["add", "--all", "--", "."])?;
	let tree = run_git_with_index(root, &index_path, &["write-tree"])?;
	let tree = tree.trim();
	let commit = Command::new("git")
		.current_dir(root)
		.env("GIT_INDEX_FILE", &index_path)
		.env("GIT_AUTHOR_NAME", "monochange")
		.env("GIT_AUTHOR_EMAIL", "monochange@localhost")
		.env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
		.env("GIT_COMMITTER_NAME", "monochange")
		.env("GIT_COMMITTER_EMAIL", "monochange@localhost")
		.env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
		.args([
			"commit-tree",
			tree,
			"-p",
			head,
			"-m",
			"monochange change-classification candidate",
		])
		.output()
		// patch-coverage:ignore-start -- git was successfully invoked three times immediately above; losing the executable or receiving non-ASCII object output here requires an OS-level fault.
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to materialize the working tree for change classification: {error}"
			))
		})?;

	if !commit.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"could not materialize the working tree for change classification: {}",
			String::from_utf8_lossy(&commit.stderr).trim()
		)));
	}

	String::from_utf8(commit.stdout)
		.map(|value| value.trim().to_string())
		.map_err(|error| {
			MonochangeError::Discovery(format!(
				"git commit-tree returned invalid utf-8 while materializing the working tree: {error}"
			))
		})
	// patch-coverage:ignore-end
}

fn run_git_with_index(root: &Path, index_path: &Path, args: &[&str]) -> MonochangeResult<String> {
	let output = Command::new("git")
		.current_dir(root)
		.env("GIT_INDEX_FILE", index_path)
		.args(args)
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to run git {args:?}: {error}")))?;

	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"git {args:?} failed while materializing the working tree: {}",
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}

	// patch-coverage:ignore-start -- git plumbing commands used here produce ASCII object ids and status text.
	String::from_utf8(output.stdout).map_err(|error| {
		MonochangeError::Discovery(format!("git {args:?} returned invalid utf-8: {error}"))
	})
	// patch-coverage:ignore-end
}

fn short_revision(revision: &str) -> String {
	revision.chars().take(12).collect()
}

fn resolve_merge_base(root: &Path, base: &str, head: &str) -> Option<String> {
	run_git(root, &["merge-base", base, head])
		.ok()
		.map(|value| value.trim().to_string())
		.filter(|value| !value.is_empty())
}

fn git_revision_exists(root: &Path, revision: &str) -> bool {
	Command::new("git")
		.current_dir(root)
		.args(["rev-parse", "--verify", revision])
		.output()
		.is_ok_and(|output| output.status.success())
}

fn run_git(root: &Path, args: &[&str]) -> MonochangeResult<String> {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.map_err(|error| MonochangeError::Io(format!("failed to run git {args:?}: {error}")))?;

	if !output.status.success() {
		return Err(MonochangeError::Discovery(format!(
			"git {args:?} failed: {}",
			String::from_utf8_lossy(&output.stderr).trim()
		)));
	}

	// patch-coverage:ignore-start -- every caller requests ASCII git refs, tags, or object ids.
	String::from_utf8(output.stdout).map_err(|error| {
		MonochangeError::Discovery(format!("git {args:?} returned invalid utf-8: {error}"))
	})
	// patch-coverage:ignore-end
}

fn working_tree_analysis(
	root: &Path,
	head: &str,
	session: &AnalysisSession,
) -> MonochangeResult<Option<ChangeAnalysis>> {
	if head != DEFAULT_HEAD_REF {
		return Ok(None);
	}

	let frame = ChangeFrame::WorkingDirectory;
	if frame.changed_files(root)?.is_empty() {
		return Ok(None);
	}

	session.analyze(&frame).map(Some)
}

fn selected_package_ids(
	root: &Path,
	packages: &[PackageRecord],
	pull_request: &ChangeAnalysis,
	options: &ClassifyOptions,
) -> MonochangeResult<BTreeSet<String>> {
	if !options.packages.is_empty() {
		let mut selected = BTreeSet::new();
		for package_reference in &options.packages {
			let record_id =
				monochange_config::resolve_package_reference(package_reference, root, packages)?;
			let package = packages
				.iter()
				.find(|package| package.id == record_id)
				.expect("resolved package record should exist");
			selected.insert(preferred_report_package_id(package));
		}

		return Ok(selected);
	}

	if options.include_unchanged {
		return Ok(packages.iter().map(preferred_report_package_id).collect());
	}

	let selected = pull_request
		.package_analyses
		.keys()
		.cloned()
		.collect::<BTreeSet<_>>();

	Ok(selected)
}

fn existing_changesets_by_package(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	packages: &[PackageRecord],
	source_delta: &ChangeAnalysis,
	working_tree: Option<&ChangeAnalysis>,
) -> MonochangeResult<ExistingChangesetInventory> {
	// patch-coverage:ignore-start -- inventory integration tests cover successful frame enumeration; llvm-cov attributes the propagated `?` expression inconsistently.
	let mut paths = source_delta
		.frame
		.changed_files(root)?
		.into_iter()
		.collect::<BTreeSet<_>>();
	// patch-coverage:ignore-end
	if let Some(working_tree) = working_tree {
		paths.extend(working_tree.frame.changed_files(root)?);
	}
	let context = monochange_config::build_changeset_load_context(configuration, packages);
	let mut changesets = BTreeMap::<String, Vec<ExistingChangeset>>::new();
	let mut warnings = Vec::new();

	for path in paths {
		if !is_changeset_path(&path) || !root.join(&path).exists() {
			continue;
		}

		match monochange_config::load_changeset_file_with_context(&root.join(&path), &context) {
			Ok(loaded) => {
				for signal in loaded.signals {
					let package_id = report_package_id_for_signal(packages, signal.package_id);
					changesets
						.entry(package_id)
						.or_default()
						.push(ExistingChangeset {
							path: path.clone(),
							bump: signal.requested_bump,
							change_type: signal.change_type,
						});
				}
			}
			Err(error) => {
				warnings.push(format!(
					"could not inspect pending changeset `{}`: {}",
					path.display(),
					error.render()
				));
			}
		}
	}

	for package_changesets in changesets.values_mut() {
		package_changesets.sort_by(|left, right| left.path.cmp(&right.path));
		package_changesets.dedup();
	}

	Ok((changesets, warnings))
}

fn report_package_id_for_signal(packages: &[PackageRecord], signal_package_id: String) -> String {
	packages
		.iter()
		.find(|package| {
			package.id == signal_package_id
				|| preferred_report_package_id(package) == signal_package_id
		})
		.map(preferred_report_package_id)
		.unwrap_or(signal_package_id)
}

fn is_changeset_path(path: &Path) -> bool {
	path.starts_with(".changeset")
		&& path
			.extension()
			.is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn latest_release_tag(
	root: &Path,
	reachable_from: &str,
	release_identity: Option<&EffectiveReleaseIdentity>,
	ecosystem: &str,
) -> MonochangeResult<Option<String>> {
	let Some(release_identity) = release_identity else {
		return Ok(None);
	};
	if !release_identity.tag {
		return Ok(None);
	}

	let output = run_git(
		root,
		&[
			"tag",
			"--merged",
			reachable_from,
			"--list",
			"--sort=-v:refname",
		],
	)?; // patch-coverage:ignore-start -- successful automatic tag discovery is exercised directly; llvm-cov attributes the propagated `?` to this call site. patch-coverage:ignore-end
	let latest = output
		.lines()
		.map(str::trim)
		.filter_map(|tag| {
			release_tag_version(
				tag,
				&release_identity.version_format,
				&release_identity.owner_id,
				ecosystem,
			)
			.map(|version| (version, tag.to_string()))
		})
		.max_by(|(left, _), (right, _)| left.cmp(right))
		.map(|(_, tag)| tag);

	Ok(latest)
}

fn release_tag_version(
	tag: &str,
	version_format: &VersionFormat,
	owner_id: &str,
	ecosystem: &str,
) -> Option<semver::Version> {
	let mut boundaries = tag
		.char_indices()
		.map(|(index, _)| index)
		.collect::<Vec<_>>();
	boundaries.push(tag.len());

	for (start_index, start) in boundaries.iter().copied().enumerate() {
		if !tag[start..].starts_with(|character: char| character.is_ascii_digit()) {
			continue;
		}
		for end in boundaries.iter().copied().skip(start_index + 1) {
			let candidate = &tag[start..end];
			let Ok(version) = semver::Version::parse(candidate) else {
				continue;
			};
			if version_format
				.render_tag(owner_id, candidate, ecosystem)
				.is_ok_and(|rendered| rendered == tag)
			{
				return Some(version);
			}
		}
	}

	None
}

fn release_owner(
	release_identity: Option<&EffectiveReleaseIdentity>,
	latest_release: Option<String>,
) -> Option<ReleaseOwner> {
	let release_identity = release_identity?;
	let kind = match release_identity.owner_kind {
		ReleaseOwnerKind::Group => "group",
		ReleaseOwnerKind::Package => "package",
	};

	Some(ReleaseOwner {
		kind: kind.to_string(),
		id: release_identity.owner_id.clone(),
		latest_release,
	})
}

fn pull_request_changed_files(package_id: &str, pull_request: &ChangeAnalysis) -> Vec<PathBuf> {
	pull_request
		.package_analyses
		.get(package_id)
		.into_iter()
		.flat_map(|package| package.changed_files.iter().cloned())
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

fn collect_findings(
	package_id: &str,
	evidence: &[PackageEvidence<'_>],
	detection_level: DetectionLevel,
) -> Vec<ClassificationFinding> {
	let mut grouped =
		BTreeMap::<String, BTreeMap<FindingEvidenceKey, PendingClassificationFinding>>::new();
	for frame in evidence {
		let Some(package) = frame.analysis.package_analyses.get(package_id) else {
			continue;
		};
		let analyzer_id = package
			.analyzer_id
			.as_deref()
			.unwrap_or("monochange/unknown-analyzer");

		for change in &package.semantic_changes {
			let analyzer_id = if change.category == SemanticChangeCategory::Package {
				"monochange/package-lifecycle"
			} else {
				analyzer_id
			};
			let base_id = finding_id(analyzer_id, change);
			let pending = grouped
				.entry(base_id)
				.or_default()
				.entry(FindingEvidenceKey::from(change))
				.or_insert_with(|| {
					PendingClassificationFinding {
						analyzer_id: analyzer_id.to_string(),
						change: change.clone(),
						comparisons: BTreeSet::new(),
					}
				});
			pending.comparisons.insert(frame.kind);
		}
	}

	grouped
		.into_iter()
		.flat_map(|(base_id, variants)| {
			let has_multiple_variants = variants.len() > 1;
			variants.into_iter().map(move |(evidence_key, pending)| {
				let id = if has_multiple_variants {
					format!("{base_id}@{:016x}", evidence_key.stable_fingerprint())
				} else {
					base_id.clone()
				};
				let mut finding = finding_from_semantic_change(
					id,
					&pending.analyzer_id,
					&pending.change,
					detection_level,
				);
				finding.comparisons = pending.comparisons;
				finding
			})
		})
		.collect()
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct FindingEvidenceKey {
	before: Option<String>,
	after: Option<String>,
	location: PathBuf,
	summary: String,
}

impl From<&SemanticChange> for FindingEvidenceKey {
	fn from(change: &SemanticChange) -> Self {
		Self {
			before: change.before_signature.clone(),
			after: change.after_signature.clone(),
			location: change.file_path.clone(),
			summary: change.summary.clone(),
		}
	}
}

impl FindingEvidenceKey {
	fn stable_fingerprint(&self) -> u64 {
		const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
		const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

		let mut hash = FNV_OFFSET_BASIS;
		let location = self.location.to_string_lossy();
		for value in [
			self.before.as_deref(),
			self.after.as_deref(),
			Some(location.as_ref()),
			Some(self.summary.as_str()),
		] {
			for byte in value.unwrap_or("\0").as_bytes() {
				hash ^= u64::from(*byte);
				hash = hash.wrapping_mul(FNV_PRIME);
			}
			hash ^= 0xff;
			hash = hash.wrapping_mul(FNV_PRIME);
		}
		hash
	}
}

struct PendingClassificationFinding {
	analyzer_id: String,
	change: SemanticChange,
	comparisons: BTreeSet<ComparisonKind>,
}

fn finding_id(analyzer_id: &str, change: &SemanticChange) -> String {
	format!(
		"{analyzer_id}/{}/{}/{}/{}",
		semantic_category_name(change.category),
		semantic_kind_name(change.kind),
		change.item_kind,
		change.item_path
	)
}

fn finding_from_semantic_change(
	id: String,
	analyzer_id: &str,
	change: &SemanticChange,
	detection_level: DetectionLevel,
) -> ClassificationFinding {
	let bump = monochange_semver::semantic_change_severity(change);
	let impact = compatibility_impact(change.category, change.kind);
	let package_lifecycle = change.category == SemanticChangeCategory::Package;
	let surface = semantic_category_name(change.category).to_string();
	let change_name = semantic_kind_name(change.kind).to_string();
	let rule_id = format!("{analyzer_id}/{surface}/{change_name}/{}", change.item_kind);

	ClassificationFinding {
		id,
		rule_id,
		surface,
		change: change_name,
		impact,
		bump,
		confidence: if package_lifecycle {
			ClassificationConfidence::High
		} else {
			ClassificationConfidence::Medium
		},
		analyzer: FindingAnalyzer {
			id: analyzer_id.to_string(),
			version: ANALYZER_VERSION.to_string(),
		},
		coverage: FindingCoverage {
			detection_level,
			completeness: if package_lifecycle {
				AnalysisCompleteness::Complete
			} else {
				AnalysisCompleteness::Partial
			},
			note: analyzer_coverage_note(analyzer_id).to_string(),
		},
		before: change.before_signature.clone(),
		after: change.after_signature.clone(),
		location: change.file_path.clone(),
		comparisons: BTreeSet::new(),
		summary: change.summary.clone(),
	}
}

fn compatibility_impact(
	category: SemanticChangeCategory,
	kind: SemanticChangeKind,
) -> CompatibilityImpact {
	match (category, kind) {
		(
			SemanticChangeCategory::Package
			| SemanticChangeCategory::PublicApi
			| SemanticChangeCategory::Export,
			SemanticChangeKind::Removed | SemanticChangeKind::Modified,
		) => CompatibilityImpact::Breaking,
		(
			SemanticChangeCategory::Package
			| SemanticChangeCategory::PublicApi
			| SemanticChangeCategory::Export,
			SemanticChangeKind::Added,
		) => CompatibilityImpact::Additive,
		(SemanticChangeCategory::Dependency | SemanticChangeCategory::Metadata, _) => {
			CompatibilityImpact::Compatible
		}
		// patch-coverage:ignore-start -- future-proof fallback for non-exhaustive semantic enums.
		_ => CompatibilityImpact::Unknown,
		// patch-coverage:ignore-end
	}
}

fn semantic_category_name(category: SemanticChangeCategory) -> &'static str {
	match category {
		SemanticChangeCategory::Package => "package",
		SemanticChangeCategory::PublicApi => "public_api",
		SemanticChangeCategory::Export => "export",
		SemanticChangeCategory::Dependency => "dependency",
		SemanticChangeCategory::Metadata => "metadata",
		// patch-coverage:ignore-start -- future-proof fallback for a non-exhaustive external enum.
		_ => "unknown",
		// patch-coverage:ignore-end
	}
}

fn semantic_kind_name(kind: SemanticChangeKind) -> &'static str {
	match kind {
		SemanticChangeKind::Added => "added",
		SemanticChangeKind::Removed => "removed",
		SemanticChangeKind::Modified => "modified",
		// patch-coverage:ignore-start -- future-proof fallback for a non-exhaustive external enum.
		_ => "unknown",
		// patch-coverage:ignore-end
	}
}

fn analyzer_coverage_note(analyzer_id: &str) -> &'static str {
	if analyzer_id == "monochange/package-lifecycle" {
		return "package manifest presence was compared at both endpoints";
	}
	if analyzer_id.starts_with("cargo/") {
		return "syntax-level Rust surface; module reachability, cfg and feature matrices, trait compatibility, and downstream witnesses are not complete";
	}
	if analyzer_id.starts_with("npm/") || analyzer_id.starts_with("deno/") {
		return "package metadata and export syntax; entrypoint reachability and TypeScript assignability are not complete";
	}
	if analyzer_id.starts_with("dart/") {
		return "Dart declaration syntax; library reachability and analyzer-level type compatibility are not complete";
	}

	"the analyzer does not declare complete compatibility coverage"
}

fn ensure_unclassified_finding(
	package_id: &str,
	ecosystem: monochange_core::Ecosystem,
	detection_level: DetectionLevel,
	changed_files: &[PathBuf],
	findings: &mut Vec<ClassificationFinding>,
) {
	let has_pull_request_finding = findings
		.iter()
		.any(|finding| finding.comparisons.contains(&ComparisonKind::PullRequest));
	if changed_files.is_empty() || has_pull_request_finding {
		return;
	}

	// patch-coverage:ignore-start -- the empty slice returns above, so first() is always Some here.
	let location = changed_files
		.first()
		.cloned()
		.unwrap_or_else(|| PathBuf::from("."));
	// patch-coverage:ignore-end
	findings.push(ClassificationFinding {
		id: format!("monochange/unclassified-source/{package_id}"),
		rule_id: "monochange/unclassified-source".to_string(),
		surface: "source".to_string(),
		change: "modified".to_string(),
		impact: CompatibilityImpact::Unknown,
		bump: BumpSeverity::Patch,
		confidence: ClassificationConfidence::Low,
		analyzer: FindingAnalyzer {
			id: format!("{}/fallback", ecosystem.as_str()),
			version: ANALYZER_VERSION.to_string(),
		},
		coverage: FindingCoverage {
			detection_level,
			completeness: AnalysisCompleteness::Partial,
			note: "changed package files produced no modeled compatibility finding".to_string(),
		},
		before: None,
		after: None,
		location,
		comparisons: [ComparisonKind::PullRequest].into_iter().collect(),
		summary: "package files changed outside the analyzer's modeled public surface".to_string(),
	});
}

fn build_recommendation(
	findings: &[ClassificationFinding],
	has_current_changes: bool,
	has_release: bool,
) -> ChangeRecommendation {
	let current = findings
		.iter()
		.filter(|finding| finding.comparisons.contains(&ComparisonKind::PullRequest))
		.collect::<Vec<_>>();
	let proposed_changeset_bump = current
		.iter()
		.map(|finding| finding.bump)
		.max()
		.unwrap_or(BumpSeverity::None);
	let enforceable_minimum = current
		.iter()
		.filter(|finding| finding.confidence == ClassificationConfidence::High)
		.map(|finding| finding.bump)
		.max()
		.unwrap_or(BumpSeverity::None);
	let release_bump = findings
		.iter()
		.filter(|finding| finding.comparisons.contains(&ComparisonKind::Release))
		.map(|finding| finding.bump)
		.max()
		.unwrap_or(BumpSeverity::None);
	let release_floor = if has_release {
		std::cmp::max(release_bump, proposed_changeset_bump)
	} else {
		proposed_changeset_bump
	};
	let compatibility_impact = highest_compatibility_impact(&current);
	let confidence = current
		.iter()
		.filter(|finding| finding.bump == proposed_changeset_bump)
		.map(|finding| finding.confidence)
		.max()
		.unwrap_or(ClassificationConfidence::High);
	let has_conclusive_major = current.iter().any(|finding| {
		finding.bump == BumpSeverity::Major
			&& finding.confidence == ClassificationConfidence::High
			&& finding.coverage.completeness == AnalysisCompleteness::Complete
	});
	let completeness = if !has_current_changes || has_conclusive_major {
		AnalysisCompleteness::Complete
	} else {
		AnalysisCompleteness::Partial
	};
	let review_required = completeness != AnalysisCompleteness::Complete
		|| compatibility_impact == CompatibilityImpact::Unknown;
	let finding_ids = current
		.iter()
		.filter(|finding| finding.bump == proposed_changeset_bump)
		.map(|finding| finding.id.clone())
		.collect();

	ChangeRecommendation {
		compatibility_impact,
		proposed_changeset_bump,
		enforceable_minimum,
		release_floor,
		confidence,
		completeness,
		review_required,
		finding_ids,
	}
}

fn highest_compatibility_impact(findings: &[&ClassificationFinding]) -> CompatibilityImpact {
	if findings
		.iter()
		.any(|finding| finding.impact == CompatibilityImpact::Breaking)
	{
		return CompatibilityImpact::Breaking;
	}
	if findings
		.iter()
		.any(|finding| finding.impact == CompatibilityImpact::Additive)
	{
		return CompatibilityImpact::Additive;
	}
	if findings
		.iter()
		.any(|finding| finding.impact == CompatibilityImpact::Unknown)
	{
		return CompatibilityImpact::Unknown;
	}

	CompatibilityImpact::Compatible
}

fn changeset_action(
	decision: &ChangeRecommendation,
	existing: &[ExistingChangeset],
) -> ChangesetAction {
	if decision.proposed_changeset_bump == BumpSeverity::None {
		return if existing.is_empty() {
			ChangesetAction::NoChangeset
		} else {
			ChangesetAction::Review
		};
	}
	if existing.is_empty() {
		return ChangesetAction::Create;
	}

	let existing_bump = existing
		.iter()
		.filter_map(|changeset| changeset.bump)
		.max()
		.unwrap_or(BumpSeverity::None);
	if existing_bump < decision.proposed_changeset_bump {
		ChangesetAction::Update
	} else {
		ChangesetAction::Keep
	}
}

fn recommendation_summary(
	decision: &ChangeRecommendation,
	findings: &[ClassificationFinding],
) -> String {
	let count = decision.finding_ids.len();
	match decision.proposed_changeset_bump {
		BumpSeverity::Major => format!("{count} breaking finding(s) propose a major changeset"),
		BumpSeverity::Minor => format!("{count} additive finding(s) propose a minor changeset"),
		BumpSeverity::Patch
			if findings
				.iter()
				.any(|finding| finding.impact == CompatibilityImpact::Unknown) =>
		{
			"unclassified package changes propose a patch changeset and require review".to_string()
		}
		BumpSeverity::Patch => format!("{count} compatible finding(s) propose a patch changeset"),
		BumpSeverity::None if decision.review_required => {
			"pending changeset intent has no matching package change and requires review"
				.to_string()
		}
		BumpSeverity::None => "no package change requires a changeset".to_string(),
		// patch-coverage:ignore-start -- future-proof fallback for a non-exhaustive external bump enum.
		_ => "the package change requires review".to_string(),
		// patch-coverage:ignore-end
	}
}

fn package_warnings(package_id: &str, evidence: &[PackageEvidence<'_>]) -> Vec<String> {
	evidence
		.iter()
		.flat_map(|frame| {
			frame
				.analysis
				.package_analyses
				.get(package_id)
				.into_iter()
				.flat_map(|package| package.warnings.iter().cloned())
		})
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

fn parse_detection_level(value: &str) -> MonochangeResult<DetectionLevel> {
	match value {
		"basic" => Ok(DetectionLevel::Basic),
		"signature" => Ok(DetectionLevel::Signature),
		"semantic" => Ok(DetectionLevel::Semantic),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported detection level `{other}`; expected basic, signature, or semantic"
			)))
		}
	}
}

fn propagate_public_dependency_impacts(
	analysis: &ChangeAnalysis,
	packages: &mut Vec<PackageClassification>,
	recommendation: &mut BumpSeverity,
) {
	for impact in public_dependency_impacts(analysis) {
		let dependent_id = preferred_report_package_id(&impact.dependent);
		let upstream_name = impact.upstream_name;
		let finding_id = format!("monochange/public-dependency/{upstream_name}");
		let finding = ClassificationFinding {
			id: finding_id.clone(),
			rule_id: "monochange/public-dependency".to_string(),
			surface: "dependency".to_string(),
			change: "propagated".to_string(),
			impact: CompatibilityImpact::Compatible,
			bump: BumpSeverity::Patch,
			confidence: ClassificationConfidence::Medium,
			analyzer: FindingAnalyzer {
				id: "monochange/dependency-propagation".to_string(),
				version: ANALYZER_VERSION.to_string(),
			},
			coverage: FindingCoverage {
				detection_level: analysis.detection_level,
				completeness: AnalysisCompleteness::Partial,
				note: "direct runtime dependency propagation".to_string(),
			},
			before: None,
			after: None,
			location: impact
				.dependent
				.relative_manifest_path(&impact.dependent.workspace_root)
				.unwrap_or_else(|| impact.dependent.manifest_path.clone()),
			comparisons: [ComparisonKind::PullRequest].into_iter().collect(),
			summary: format!(
				"public dependency `{upstream_name}` changed; verify re-exports and constraints"
			),
		};

		if let Some(package) = packages
			.iter_mut()
			.find(|package| package.package_id == dependent_id)
		{
			if !package
				.findings
				.iter()
				.any(|existing| existing.id == finding.id)
			{
				package.findings.push(finding);
			}
			if package.decision.proposed_changeset_bump < BumpSeverity::Patch {
				package.decision.compatibility_impact = CompatibilityImpact::Compatible;
				package.decision.proposed_changeset_bump = BumpSeverity::Patch;
				package.decision.confidence = ClassificationConfidence::Medium;
				package.decision.finding_ids = vec![finding_id];
				package.recommendation = BumpSeverity::Patch;
				package.summary = format!(
					"public dependency `{upstream_name}` changed; verify the dependent package still re-exports or constrains it correctly"
				);
			} else if package.decision.proposed_changeset_bump == BumpSeverity::Patch
				&& !package.decision.finding_ids.contains(&finding_id)
			{
				package.decision.finding_ids.push(finding_id);
			}
			package.decision.release_floor =
				std::cmp::max(package.decision.release_floor, BumpSeverity::Patch);
			package.decision.completeness = AnalysisCompleteness::Partial;
			package.decision.review_required = true;
			package.action = changeset_action(&package.decision, &package.existing_changesets);
			*recommendation = std::cmp::max(*recommendation, package.recommendation);
			continue;
		}

		let decision = ChangeRecommendation {
			compatibility_impact: CompatibilityImpact::Compatible,
			proposed_changeset_bump: BumpSeverity::Patch,
			enforceable_minimum: BumpSeverity::None,
			release_floor: BumpSeverity::Patch,
			confidence: ClassificationConfidence::Medium,
			completeness: AnalysisCompleteness::Partial,
			review_required: true,
			finding_ids: vec![finding_id],
		};
		packages.push(PackageClassification {
			package_id: dependent_id,
			package_name: impact.dependent.name,
			ecosystem: impact.dependent.ecosystem,
			release_owner: None,
			comparisons: Vec::new(),
			recommendation: BumpSeverity::Patch,
			decision,
			summary: format!(
				"public dependency `{upstream_name}` changed; verify the dependent package still re-exports or constrains it correctly"
			),
			findings: vec![finding],
			existing_changesets: Vec::new(),
			action: ChangesetAction::Create,
			warnings: Vec::new(),
		});
		*recommendation = std::cmp::max(*recommendation, BumpSeverity::Patch);
	}

	packages.sort_by(|left, right| left.package_id.cmp(&right.package_id));
}

fn public_dependency_impacts(analysis: &ChangeAnalysis) -> Vec<PublicDependencyImpact> {
	let mut impacted_record_ids = BTreeMap::new();
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
		return Vec::new();
	}

	let package_by_record_id = analysis
		.packages
		.iter()
		.map(|package| (package.id.clone(), package))
		.collect::<BTreeMap<_, _>>();
	let mut impacts = Vec::new();
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
		let upstream_name = impacted_record_ids
			.get(&edge.to_package_id)
			.cloned()
			.unwrap_or(edge.to_package_id.clone());
		impacts.push(PublicDependencyImpact {
			dependent: (*dependent).clone(),
			upstream_name,
		});
	}

	impacts.sort_by(|left, right| {
		preferred_report_package_id(&left.dependent)
			.cmp(&preferred_report_package_id(&right.dependent))
			.then_with(|| left.upstream_name.cmp(&right.upstream_name))
	});
	impacts.dedup_by(|left, right| {
		preferred_report_package_id(&left.dependent)
			== preferred_report_package_id(&right.dependent)
			&& left.upstream_name == right.upstream_name
	});
	impacts
}

fn preferred_report_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

#[coverage(off)]
fn render_markdown_report(report: &ChangeClassificationReport) -> String {
	let mut lines = vec!["# Change classification".to_string(), String::new()];
	lines.push(format!("- Schema version: `{}`", report.schema_version));
	lines.push(format!("- Default branch: `{}`", report.default_branch));
	lines.push(format!("- Candidate: `{}`", report.candidate));
	lines.push(format!("- Recommended bump: `{}`", report.recommendation));
	lines.push(format!("- Packages analyzed: {}", report.packages.len()));
	lines.push(String::new());
	lines.push("## Comparisons".to_string());
	lines.push(String::new());
	for comparison in &report.comparisons {
		let base = comparison.base.as_deref().unwrap_or("unavailable");
		lines.push(format!(
			"- `{}`: `{base}` to `{}` ({})",
			comparison_kind_name(comparison.kind),
			comparison.head,
			comparison_status_name(comparison.status)
		));
	}
	lines.push(String::new());

	if report.packages.is_empty() {
		lines.push("No package changes were detected for the pull request comparison.".to_string());
	} else {
		lines.push("## Packages".to_string());
		lines.push(String::new());
		for package in &report.packages {
			lines.push(format!("### `{}`", package.package_id));
			lines.push(String::new());
			lines.push(format!("- Ecosystem: `{}`", package.ecosystem));
			lines.push(format!(
				"- Proposed changeset bump: `{}`",
				package.decision.proposed_changeset_bump
			));
			lines.push(format!(
				"- Enforceable minimum: `{}`",
				package.decision.enforceable_minimum
			));
			lines.push(format!(
				"- Release floor: `{}`",
				package.decision.release_floor
			));
			lines.push(
				format!(
					"- Compatibility impact: `{:?}`",
					package.decision.compatibility_impact
				)
				.to_lowercase(),
			);
			lines.push(format!("- Confidence: `{:?}`", package.decision.confidence).to_lowercase());
			lines.push(
				format!("- Completeness: `{:?}`", package.decision.completeness).to_lowercase(),
			);
			lines.push(format!(
				"- Review required: `{}`",
				package.decision.review_required
			));
			lines.push(format!("- Changeset action: `{:?}`", package.action).to_lowercase());
			lines.push(format!("- Summary: {}", package.summary));
			if let Some(owner) = &package.release_owner {
				lines.push(format!("- Release owner: `{}` `{}`", owner.kind, owner.id));
				lines.push(format!(
					"- Latest release: `{}`",
					owner.latest_release.as_deref().unwrap_or("none")
				));
			}
			if !package.existing_changesets.is_empty() {
				lines.push("- Existing changesets:".to_string());
				for changeset in &package.existing_changesets {
					lines.push(format!(
						"  - `{}`: `{}`",
						changeset.path.display(),
						changeset
							.bump
							.map_or_else(|| "custom".to_string(), |bump| bump.to_string())
					));
				}
			}
			lines.push(format!("- Findings: {}", package.findings.len()));
			if !package.findings.is_empty() {
				lines.push(String::new());
				for finding in package.findings.iter().take(10) {
					let comparisons = finding
						.comparisons
						.iter()
						.map(|comparison| comparison_kind_name(*comparison))
						.collect::<Vec<_>>()
						.join(", ");
					lines.push(format!(
						"- `{}`: {} (`{}`, impact `{}`, bump `{}`, confidence `{}`, comparisons `{comparisons}`)",
						finding.id,
						finding.summary,
						finding.location.display(),
						compatibility_impact_name(finding.impact),
						finding.bump,
						classification_confidence_name(finding.confidence)
					));
				}
			}
			if package.findings.len() > 10 {
				lines.push(format!("- {} more findings", package.findings.len() - 10));
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

#[coverage(off)]
fn render_text_report(report: &ChangeClassificationReport) -> String {
	let mut lines = vec!["Change classification".to_string(), String::new()];
	lines.push(format!("Schema version: {}", report.schema_version));
	lines.push(format!("Default branch: {}", report.default_branch));
	lines.push(format!("Candidate: {}", report.candidate));
	lines.push(format!("Recommended bump: {}", report.recommendation));
	lines.push(format!("Packages analyzed: {}", report.packages.len()));
	lines.push(String::new());
	lines.push("Comparisons".to_string());
	for comparison in &report.comparisons {
		let base = comparison.base.as_deref().unwrap_or("unavailable");
		lines.push(format!(
			"  {}: {base} to {} ({})",
			comparison_kind_name(comparison.kind),
			comparison.head,
			comparison_status_name(comparison.status)
		));
	}

	if report.packages.is_empty() {
		lines.push(String::new());
		lines.push("No package changes were detected for the pull request comparison.".to_string());
	} else {
		lines.push(String::new());
		lines.push("Packages".to_string());
		for package in &report.packages {
			lines.push(String::new());
			lines.push(package.package_id.clone());
			lines.push(format!("  Ecosystem: {}", package.ecosystem));
			lines.push(format!(
				"  Proposed changeset bump: {}",
				package.decision.proposed_changeset_bump
			));
			lines.push(format!(
				"  Enforceable minimum: {}",
				package.decision.enforceable_minimum
			));
			lines.push(format!(
				"  Release floor: {}",
				package.decision.release_floor
			));
			lines.push(format!(
				"  Compatibility impact: {}",
				compatibility_impact_name(package.decision.compatibility_impact)
			));
			lines.push(format!(
				"  Confidence: {}",
				classification_confidence_name(package.decision.confidence)
			));
			lines.push(
				format!("  Completeness: {:?}", package.decision.completeness).to_lowercase(),
			);
			lines.push(format!(
				"  Review required: {}",
				package.decision.review_required
			));
			lines.push(format!("  Changeset action: {:?}", package.action).to_lowercase());
			lines.push(format!(
				"  Summary: {}",
				plain_text_fragment(&package.summary)
			));
			if let Some(owner) = &package.release_owner {
				lines.push(format!("  Release owner: {} {}", owner.kind, owner.id));
				lines.push(format!(
					"  Latest release: {}",
					owner.latest_release.as_deref().unwrap_or("none")
				));
			}
			if !package.existing_changesets.is_empty() {
				lines.push("  Existing changesets:".to_string());
				for changeset in &package.existing_changesets {
					lines.push(format!(
						"    {}: {}",
						changeset.path.display(),
						changeset
							.bump
							.map_or_else(|| "custom".to_string(), |bump| bump.to_string())
					));
				}
			}
			lines.push(format!("  Findings: {}", package.findings.len()));
			for finding in package.findings.iter().take(10) {
				let comparisons = finding
					.comparisons
					.iter()
					.map(|comparison| comparison_kind_name(*comparison))
					.collect::<Vec<_>>()
					.join(", ");
				lines.push(format!(
					"  - {}: {} ({}, impact {}, bump {}, confidence {}, comparisons {comparisons})",
					finding.id,
					plain_text_fragment(&finding.summary),
					finding.location.display(),
					compatibility_impact_name(finding.impact),
					finding.bump,
					classification_confidence_name(finding.confidence)
				));
			}
			if package.findings.len() > 10 {
				lines.push(format!("  - {} more findings", package.findings.len() - 10));
			}
		}
	}

	if !report.warnings.is_empty() {
		lines.push(String::new());
		lines.push("Warnings".to_string());
		lines.extend(
			report
				.warnings
				.iter()
				.map(|warning| format!("- {}", plain_text_fragment(warning))),
		);
	}

	lines.join("\n")
}

fn plain_text_fragment(value: &str) -> String {
	value.replace('`', "")
}

fn comparison_kind_name(kind: ComparisonKind) -> &'static str {
	match kind {
		ComparisonKind::PullRequest => "pullRequest",
		ComparisonKind::Release => "release",
		ComparisonKind::ReleaseToDefault => "releaseToDefault",
		ComparisonKind::SourceDelta => "sourceDelta",
		ComparisonKind::WorkingTree => "workingTree",
	}
}

fn comparison_status_name(status: ComparisonStatus) -> &'static str {
	match status {
		ComparisonStatus::Analyzed => "analyzed",
		ComparisonStatus::Unavailable => "unavailable",
		ComparisonStatus::Conflicted => "conflicted",
	}
}

fn compatibility_impact_name(impact: CompatibilityImpact) -> &'static str {
	match impact {
		CompatibilityImpact::Unknown => "unknown",
		CompatibilityImpact::Compatible => "compatible",
		CompatibilityImpact::Additive => "additive",
		CompatibilityImpact::Breaking => "breaking",
	}
}

fn classification_confidence_name(confidence: ClassificationConfidence) -> &'static str {
	match confidence {
		ClassificationConfidence::Low => "low",
		ClassificationConfidence::Medium => "medium",
		ClassificationConfidence::High => "high",
	}
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

#[cfg(test)]
#[path = "__tests__/change_classify_tests.rs"]
mod tests;
