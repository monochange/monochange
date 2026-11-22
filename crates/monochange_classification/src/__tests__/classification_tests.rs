use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::PackageDependency;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use monochange_core::VersionSource;
use monochange_test_helpers::git;
use tempfile::tempdir;

use super::*;

fn init_classification_repo() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("README.md"), "base\n")
		.unwrap_or_else(|error| panic!("write base file: {error}"));
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	git(tempdir.path(), &["branch", "-M", "main"]);

	tempdir
}

#[test]
fn git_candidate_helpers_cover_default_conflict_and_error_paths() {
	let tempdir = init_classification_repo();
	let root = tempdir.path();
	assert_eq!(resolve_default_branch_ref(root).unwrap(), "main");
	assert!(git_revision_exists(root, "main"));
	assert!(!git_revision_exists(root, "missing"));
	assert!(run_git(root, &["rev-parse", "missing"]).is_err());

	git(root, &["remote", "add", "origin", "."]);
	git(root, &["fetch", "origin", "main"]);
	git(root, &["remote", "set-head", "origin", "main"]);
	assert_eq!(resolve_default_branch_ref(root).unwrap(), "origin/main");

	let session = AnalysisSession::new(root, AnalysisConfig::default())
		.unwrap_or_else(|error| panic!("create analysis session: {error}"));
	assert!(
		working_tree_analysis(root, "main", &session)
			.unwrap_or_else(|error| panic!("skip working-tree analysis: {error}"))
			.is_none()
	);

	git(root, &["checkout", "-b", "feature"]);
	fs::write(root.join("README.md"), "feature\n")
		.unwrap_or_else(|error| panic!("write feature: {error}"));
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "feature"]);
	git(root, &["checkout", "main"]);
	fs::write(root.join("README.md"), "main\n")
		.unwrap_or_else(|error| panic!("write main: {error}"));
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main"]);

	let candidate = resolve_candidate(root, "main", "feature")
		.unwrap_or_else(|error| panic!("resolve conflicted candidate: {error}"));
	assert_eq!(candidate.status, ComparisonStatus::Conflicted);
	assert_eq!(candidate.reference, "feature");
	assert!(
		candidate
			.note
			.is_some_and(|note| note.contains("falls back"))
	);

	let missing = root.join("missing");
	assert!(resolve_default_branch_ref(&missing).is_err());
	let index = root.join("test-index");
	assert!(run_git_with_index(root, &index, &["rev-parse", "missing"]).is_err());
}

#[test]
fn selection_release_and_render_helpers_cover_every_supported_variant() {
	let root = Path::new("/repo");
	let mut package = npm_package("@acme/core", "/repo/packages/core/package.json");
	package
		.metadata
		.insert("config_id".to_string(), "core".to_string());
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: DetectionLevel::Signature,
		package_analyses: [("core".to_string(), package_with_changes("core", Vec::new()))]
			.into_iter()
			.collect(),
		warnings: Vec::new(),
		packages: vec![package.clone()],
	};
	let explicit = ClassifyOptions {
		packages: vec!["core".to_string()],
		..ClassifyOptions::default()
	};
	assert_eq!(
		selected_package_ids(root, &[package.clone()], &analysis, &explicit).unwrap(),
		["core".to_string()].into_iter().collect()
	);
	let all = ClassifyOptions {
		include_unchanged: true,
		..ClassifyOptions::default()
	};
	assert_eq!(
		selected_package_ids(root, &[package], &analysis, &all).unwrap(),
		["core".to_string()].into_iter().collect()
	);

	assert_eq!(
		comparison_kind_name(ComparisonKind::PullRequest),
		"pullRequest"
	);
	assert_eq!(comparison_kind_name(ComparisonKind::Release), "release");
	assert_eq!(
		comparison_kind_name(ComparisonKind::ReleaseToDefault),
		"releaseToDefault"
	);
	assert_eq!(
		comparison_kind_name(ComparisonKind::SourceDelta),
		"sourceDelta"
	);
	assert_eq!(
		comparison_kind_name(ComparisonKind::WorkingTree),
		"workingTree"
	);
	assert_eq!(
		comparison_status_name(ComparisonStatus::Analyzed),
		"analyzed"
	);
	assert_eq!(
		comparison_status_name(ComparisonStatus::Unavailable),
		"unavailable"
	);
	assert_eq!(
		comparison_status_name(ComparisonStatus::Conflicted),
		"conflicted"
	);

	assert_eq!(
		compatibility_impact_name(CompatibilityImpact::Unmodeled),
		"unmodeled"
	);
	assert_eq!(
		compatibility_impact_name(CompatibilityImpact::Compatible),
		"compatible"
	);
	assert_eq!(
		compatibility_impact_name(CompatibilityImpact::Additive),
		"additive"
	);
	assert_eq!(
		compatibility_impact_name(CompatibilityImpact::Breaking),
		"breaking"
	);
	assert_eq!(
		classification_confidence_name(ClassificationConfidence::Low),
		"low"
	);
	assert_eq!(
		classification_confidence_name(ClassificationConfidence::Medium),
		"medium"
	);
	assert_eq!(
		classification_confidence_name(ClassificationConfidence::High),
		"high"
	);
}

#[test]
fn latest_release_tag_uses_effective_release_identity() {
	let tempdir = init_classification_repo();
	let root = tempdir.path();
	git(root, &["tag", "core/v1.0.0"]);
	git(root, &["tag", "core/v2.0.0"]);
	git(root, &["tag", "other/v3.0.0"]);

	let enabled = EffectiveReleaseIdentity {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		group_id: None,
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		version_source: VersionSource::default(),
		initial_version: None,
		floating_tags: Vec::new(),
		members: vec!["core".to_string()],
		bump_ceiling: None,
		classification_enforced: true,
	};
	assert_eq!(
		latest_release_tag(root, "main", None, "cargo").unwrap(),
		None
	);
	assert_eq!(
		latest_release_tag(root, "main", Some(&enabled), "cargo").unwrap(),
		Some("core/v2.0.0".to_string())
	);

	let disabled = EffectiveReleaseIdentity {
		tag: false,
		..enabled
	};
	assert_eq!(
		latest_release_tag(root, "main", Some(&disabled), "cargo").unwrap(),
		None
	);
}

#[test]
fn changeset_signal_ids_normalize_to_report_package_ids() {
	let mut package = npm_package("@acme/core", "/repo/packages/core/package.json");
	package
		.metadata
		.insert("config_id".to_string(), "core".to_string());

	assert_eq!(
		report_package_id_for_signal(&[package.clone()], package.id.clone()),
		"core"
	);
	assert_eq!(
		report_package_id_for_signal(&[package], "core".to_string()),
		"core"
	);
	assert_eq!(
		report_package_id_for_signal(&[], "external".to_string()),
		"external"
	);
}

#[test]
fn public_dependency_propagation_recommends_dependent_patch_bump() {
	let core = PackageRecord::new(
		Ecosystem::Npm,
		"@acme/core",
		PathBuf::from("/repo/packages/core/package.json"),
		PathBuf::from("/repo"),
		None,
		PublishState::Public,
	);
	let core_id = core.id.clone();
	let mut app = PackageRecord::new(
		Ecosystem::Npm,
		"@acme/app",
		PathBuf::from("/repo/packages/app/package.json"),
		PathBuf::from("/repo"),
		None,
		PublishState::Public,
	);
	app.declared_dependencies.push(PackageDependency {
		name: "@acme/core".to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("workspace:*".to_string()),
		optional: false,
		source_field: Some("dependencies".to_string()),
	});
	app.declared_dependencies.push(PackageDependency {
		name: "@acme/core".to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("workspace:*".to_string()),
		optional: false,
		source_field: Some("dependencies".to_string()),
	});
	let app_id = app.id.clone();
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Signature,
		package_analyses: [(
			core_id.clone(),
			package_with_changes(&core_id, vec![added_export_change()]),
		)]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: vec![core, app],
	};

	let report = classification_report(&analysis, DependencyPropagation::Public);

	assert_eq!(report.recommendation, BumpSeverity::Minor);
	assert_package_recommendation_in_report(&report, &core_id, BumpSeverity::Minor);
	assert_package_recommendation_in_report(&report, &app_id, BumpSeverity::Patch);
	let propagated = report
		.packages
		.iter()
		.find(|package| package.package_id == app_id)
		.unwrap_or_else(|| panic!("expected propagated @acme/app package: {report:#?}"));
	assert!(
		propagated.summary.contains("public dependency"),
		"unexpected propagated summary: {}",
		propagated.summary
	);
}

#[test]
fn public_dependency_propagation_skips_non_public_or_already_reported_edges() {
	let core = npm_package("@acme/core", "/repo/packages/core/package.json");
	let core_id = core.id.clone();
	let utils = npm_package("@acme/utils", "/repo/packages/utils/package.json");
	let mut app = npm_package("@acme/app", "/repo/packages/app/package.json");
	let app_id = app.id.clone();
	app.declared_dependencies
		.push(dependency_on("@acme/core", DependencyKind::Runtime));
	app.declared_dependencies
		.push(dependency_on("@acme/utils", DependencyKind::Runtime));
	let mut docs = npm_package("@acme/docs", "/repo/packages/docs/package.json");

	docs.declared_dependencies
		.push(dependency_on("@acme/core", DependencyKind::Development));
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Signature,
		package_analyses: [
			(
				core_id.clone(),
				package_with_changes(&core_id, vec![added_export_change()]),
			),
			(
				app_id.clone(),
				package_with_changes(&app_id, vec![patch_dependency_change()]),
			),
		]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: vec![core, utils, app, docs],
	};

	let report = classification_report(&analysis, DependencyPropagation::Public);

	assert_eq!(report.packages.len(), 2);
	assert_package_recommendation_in_report(&report, &core_id, BumpSeverity::Minor);
	assert_package_recommendation_in_report(&report, &app_id, BumpSeverity::Patch);
}

#[test]
fn public_dependency_propagation_preserves_an_existing_package_report() {
	let core = npm_package("@acme/core", "/repo/packages/core/package.json");
	let core_id = core.id.clone();
	let mut app = npm_package("@acme/app", "/repo/packages/app/package.json");
	let app_id = app.id.clone();
	app.declared_dependencies
		.push(dependency_on("@acme/core", DependencyKind::Runtime));
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Signature,
		package_analyses: [(
			core_id.clone(),
			package_with_changes(&core_id, vec![added_export_change()]),
		)]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: vec![core, app],
	};
	let existing_changeset = ExistingChangeset {
		path: PathBuf::from(".changeset/app.md"),
		bump: Some(BumpSeverity::Patch),
		change_type: Some("patch".to_string()),
	};
	let comparison = ResolvedComparison {
		kind: ComparisonKind::Release,
		base: Some("v1.0.0".to_string()),
		head: "HEAD".to_string(),
		status: ComparisonStatus::Analyzed,
		note: None,
	};

	let mut packages = vec![PackageClassification {
		cli: None,
		package_id: app_id.clone(),
		package_name: "@acme/app".to_string(),
		ecosystem: Ecosystem::Npm,
		release_owner: Some(ReleaseOwner {
			kind: "group".to_string(),
			id: "workspace".to_string(),
			latest_release: Some("v1.0.0".to_string()),
		}),
		comparisons: vec![comparison.clone()],
		recommendation: BumpSeverity::None,
		decision: no_change_recommendation(),
		summary: "no package change requires a changeset".to_string(),
		findings: Vec::new(),
		existing_changesets: vec![existing_changeset.clone()],
		action: ChangesetAction::Review,
		warnings: Vec::new(),
	}];
	let mut recommendation = BumpSeverity::None;

	propagate_public_dependency_impacts(&analysis, &mut packages, &mut recommendation);

	assert_eq!(recommendation, BumpSeverity::Patch);
	assert_eq!(packages.len(), 1);
	let package = &packages[0];
	assert_eq!(package.package_id, app_id);
	assert_eq!(package.comparisons, vec![comparison]);
	assert_eq!(package.existing_changesets, vec![existing_changeset]);
	assert_eq!(package.recommendation, BumpSeverity::Patch);
	assert_eq!(package.action, ChangesetAction::Keep);

	assert_eq!(package.findings.len(), 1);
	assert_eq!(
		package
			.release_owner
			.as_ref()
			.map(|owner| owner.id.as_str()),
		Some("workspace")
	);
}

#[test]
fn fallback_findings_and_summaries_make_uncertainty_explicit() {
	let mut findings = Vec::new();
	ensure_unclassified_finding(
		"core",
		Ecosystem::Cargo,
		DetectionLevel::Basic,
		&[PathBuf::from("src/internal.rs")],
		&mut findings,
	);
	assert_eq!(findings.len(), 1);
	assert_eq!(findings[0].impact, CompatibilityImpact::Unmodeled);
	assert_eq!(findings[0].bump, BumpSeverity::Patch);
	assert_eq!(findings[0].confidence, ClassificationConfidence::Low);

	ensure_unclassified_finding(
		"core",
		Ecosystem::Cargo,
		DetectionLevel::Basic,
		&[PathBuf::from("src/internal.rs")],
		&mut findings,
	);
	assert_eq!(
		findings.len(),
		1,
		"a pull-request finding suppresses fallback duplication"
	);
	assert_eq!(
		highest_compatibility_impact(&[&findings[0]]),
		CompatibilityImpact::Unmodeled
	);

	let patch = build_recommendation(&findings, true, false, None, true);
	assert!(recommendation_summary(&patch, &findings, 0).contains("unclassified"));
	let mut compatible_findings = findings.clone();
	compatible_findings[0].impact = CompatibilityImpact::Compatible;
	let compatible_patch = build_recommendation(&compatible_findings, true, false, None, true);
	assert!(
		recommendation_summary(&compatible_patch, &compatible_findings, 0).contains("compatible")
	);

	let no_change = no_change_recommendation();
	assert_eq!(
		recommendation_summary(&no_change, &[], 0),
		"no package change requires a changeset"
	);
	let mut review = no_change;
	review.review_required = true;
	assert!(recommendation_summary(&review, &[], 0).contains("requires review"));

	assert!(analyzer_coverage_note("npm/exports").contains("TypeScript assignability"));
	assert!(analyzer_coverage_note("deno/exports").contains("TypeScript assignability"));
	assert!(analyzer_coverage_note("dart/public-api").contains("Dart declaration"));
	assert!(analyzer_coverage_note("custom/analyzer").contains("does not declare"));
}

fn assert_package_recommendation_in_report(
	report: &ChangeClassificationReport,
	package_id: &str,
	expected: BumpSeverity,
) {
	let package = report
		.packages
		.iter()
		.find(|package| package.package_id == package_id)
		.unwrap_or_else(|| panic!("missing package {package_id}: {report:#?}"));
	assert_eq!(package.recommendation, expected);
}

fn package_with_changes(
	package_id: &str,
	semantic_changes: Vec<SemanticChange>,
) -> monochange_analysis::PackageChangeAnalysis {
	monochange_analysis::PackageChangeAnalysis {
		package_id: package_id.to_string(),
		package_record_id: package_id.to_string(),
		package_name: package_id.to_string(),
		ecosystem: Ecosystem::Cargo,
		release_identity: None,
		analyzer_id: Some("cargo/public-api".to_string()),
		changed_files: vec![PathBuf::from("src/lib.rs")],
		semantic_changes,
		warnings: Vec::new(),
	}
}

fn npm_package(name: &str, manifest_path: &str) -> PackageRecord {
	PackageRecord::new(
		Ecosystem::Npm,
		name,
		PathBuf::from(manifest_path),
		PathBuf::from("/repo"),
		None,
		PublishState::Public,
	)
}

fn dependency_on(name: &str, kind: DependencyKind) -> PackageDependency {
	PackageDependency {
		name: name.to_string(),
		kind,
		version_constraint: Some("workspace:*".to_string()),
		optional: false,
		source_field: Some("dependencies".to_string()),
	}
}

fn no_change_recommendation() -> ChangeRecommendation {
	ChangeRecommendation {
		compatibility_impact: CompatibilityImpact::Compatible,
		release_impact: None,
		pull_request_changes: false,
		proposed_changeset_bump: BumpSeverity::None,
		enforceable_minimum: BumpSeverity::None,
		release_floor: BumpSeverity::None,
		confidence: ClassificationConfidence::High,
		completeness: AnalysisCompleteness::Complete,
		review_required: false,
		finding_ids: Vec::new(),
		classification_enforced: true,
	}
}

fn removed_api_change() -> SemanticChange {
	SemanticChange::new(
		SemanticChangeCategory::PublicApi,
		SemanticChangeKind::Removed,
		"function",
		"crate::old",
		"removed public function `crate::old`",
		PathBuf::from("src/lib.rs"),
	)
	.with_before_signature("pub fn old()")
}

fn modified_api_change() -> SemanticChange {
	SemanticChange::new(
		SemanticChangeCategory::PublicApi,
		SemanticChangeKind::Modified,
		"function",
		"crate::refined",
		"modified public function `crate::refined`",
		PathBuf::from("src/lib.rs"),
	)
	.with_before_signature("pub fn refined()")
	.with_after_signature("pub fn refined(value: String)")
}

fn added_export_change() -> SemanticChange {
	SemanticChange::new(
		SemanticChangeCategory::Export,
		SemanticChangeKind::Added,
		"function",
		"render",
		"added export `render`",
		PathBuf::from("src/index.ts"),
	)
	.with_after_signature("export function render()")
}

fn patch_dependency_change() -> SemanticChange {
	SemanticChange::new(
		SemanticChangeCategory::Dependency,
		SemanticChangeKind::Modified,
		"dependency",
		"serde",
		"changed dependency `serde`",
		PathBuf::from("Cargo.toml"),
	)
	.with_before_signature("serde = 1")
	.with_after_signature("serde = 1.0.1")
}

#[test]
fn semantic_assessment_mapping_covers_every_current_evidence_variant() {
	assert_eq!(
		compatibility_impact_from_outcome(monochange_core::SemanticAnalysisOutcome::Additive),
		CompatibilityImpact::Additive
	);
	assert_eq!(
		compatibility_impact_from_outcome(monochange_core::SemanticAnalysisOutcome::Breaking),
		CompatibilityImpact::Breaking
	);
	assert_eq!(
		compatibility_impact_from_outcome(monochange_core::SemanticAnalysisOutcome::Inconclusive),
		CompatibilityImpact::Unmodeled
	);
	assert_eq!(
		classification_confidence(monochange_core::ApiConfidence::Medium),
		ClassificationConfidence::Medium
	);
	assert_eq!(
		classification_confidence(monochange_core::ApiConfidence::Low),
		ClassificationConfidence::Low
	);
	assert_eq!(
		analysis_completeness(monochange_core::SemanticAnalysisCompleteness::Partial),
		AnalysisCompleteness::Partial
	);
	assert_eq!(
		analysis_completeness(monochange_core::SemanticAnalysisCompleteness::Unsupported),
		AnalysisCompleteness::Unsupported
	);
	assert_eq!(
		analysis_completeness_name(AnalysisCompleteness::Complete),
		"complete"
	);

	assert_eq!(
		analysis_completeness_name(AnalysisCompleteness::Unsupported),
		"unsupported"
	);
}

#[test]
fn semantic_finding_preserves_and_renders_matrix_checks() {
	let mut change = removed_api_change();
	change.assessment = Some(monochange_core::SemanticChangeAssessment::new(
		monochange_core::SemanticAnalysisOutcome::Breaking,
		BumpSeverity::Major,
		monochange_core::ApiConfidence::High,
		monochange_core::SemanticAnalyzerEvidence::new(
			"cargo/cargo-semver-checks",
			"cargo-semver-checks",
			monochange_core::SemanticAnalysisCompleteness::Complete,
			"2/2 feature/target cells checked",
		)
		.with_version("0.47.0")
		.with_checks(vec![
			monochange_core::SemanticAnalyzerCheck::new(
				"all-features",
				monochange_core::SemanticAnalyzerCheckStatus::Checked,
				BTreeMap::from([("target".to_string(), "host".to_string())]),
			)
			.with_result(
				monochange_core::SemanticAnalysisOutcome::Breaking,
				BumpSeverity::Major,
			)
			.with_diagnostics(vec![monochange_core::SemanticAnalyzerDiagnostic::new(
				"trait_method_missing",
				"pub trait method removed",
			)]),
		]),
	));
	let finding = finding_from_semantic_change(
		finding_id("cargo/public-api", &change),
		"cargo/public-api",
		&change,
		DetectionLevel::Semantic,
	);
	let json =
		serde_json::to_value(&finding).unwrap_or_else(|error| panic!("serialize finding: {error}"));

	assert_eq!(json["coverage"]["checks"][0]["name"], "all-features");
	assert_eq!(json["coverage"]["checks"][0]["suggested_bump"], "major");
	assert!(
		finding_evidence(&finding).contains(
			"all-features=checked/major [trait_method_missing: pub trait method removed]"
		)
	);
}

#[test]
fn matrix_evidence_renders_every_status_and_bump() {
	let checks = vec![
		SemanticAnalyzerCheck::new(
			"checked",
			SemanticAnalyzerCheckStatus::Checked,
			BTreeMap::new(),
		),
		SemanticAnalyzerCheck::new(
			"none",
			SemanticAnalyzerCheckStatus::Checked,
			BTreeMap::new(),
		)
		.with_result(SemanticAnalysisOutcome::Compatible, BumpSeverity::None),
		SemanticAnalyzerCheck::new(
			"patch",
			SemanticAnalyzerCheckStatus::Checked,
			BTreeMap::new(),
		)
		.with_result(SemanticAnalysisOutcome::Compatible, BumpSeverity::Patch),
		SemanticAnalyzerCheck::new(
			"minor",
			SemanticAnalyzerCheckStatus::Checked,
			BTreeMap::new(),
		)
		.with_result(SemanticAnalysisOutcome::Additive, BumpSeverity::Minor),
		SemanticAnalyzerCheck::new(
			"skipped",
			SemanticAnalyzerCheckStatus::Skipped,
			BTreeMap::new(),
		),
		SemanticAnalyzerCheck::new(
			"failed",
			SemanticAnalyzerCheckStatus::Failed,
			BTreeMap::new(),
		),
	];

	assert_eq!(
		finding_checks_evidence(&checks),
		"; checks: checked=checked; none=checked/none; patch=checked/patch; minor=checked/minor; skipped=skipped; failed=failed"
	);
}

#[test]
fn finding_fingerprint_includes_the_semantic_assessment() {
	let mut change = removed_api_change();
	let without_assessment = FindingEvidenceKey::from(&change).stable_fingerprint();
	change.assessment = Some(monochange_core::SemanticChangeAssessment::new(
		monochange_core::SemanticAnalysisOutcome::Breaking,
		BumpSeverity::Major,
		monochange_core::ApiConfidence::High,
		monochange_core::SemanticAnalyzerEvidence::new(
			"npm/typescript",
			"typescript",
			monochange_core::SemanticAnalysisCompleteness::Complete,
			"all exports",
		),
	));

	assert_ne!(
		FindingEvidenceKey::from(&change).stable_fingerprint(),
		without_assessment
	);
}

#[test]
fn package_lifecycle_findings_are_complete_and_enforceable() {
	let removed = SemanticChange::new(
		SemanticChangeCategory::Package,
		SemanticChangeKind::Removed,
		"package",
		"retired",
		"removed cargo package `retired`",
		PathBuf::from("Cargo.toml"),
	)
	.with_before_signature("cargo package `retired`");
	let finding = finding_from_semantic_change(
		finding_id("monochange/package-lifecycle", &removed),
		"monochange/package-lifecycle",
		&removed,
		monochange_analysis::DetectionLevel::Signature,
	);

	assert_eq!(finding.impact, CompatibilityImpact::Breaking);
	assert_eq!(finding.bump, BumpSeverity::Major);
	assert_eq!(finding.confidence, ClassificationConfidence::High);
	assert_eq!(
		finding.coverage.completeness,
		AnalysisCompleteness::Complete
	);
	assert_eq!(finding.surface, "package");
	let mut finding = finding;
	finding.comparisons.insert(ComparisonKind::PullRequest);
	let decision = build_recommendation(&[finding.clone()], true, false, None, true);

	assert_eq!(decision.enforceable_minimum, BumpSeverity::Major);
	assert_eq!(decision.completeness, AnalysisCompleteness::Complete);
	assert!(!decision.review_required);

	finding.coverage.completeness = AnalysisCompleteness::Partial;
	let partial_decision = build_recommendation(&[finding], true, false, None, true);
	assert_eq!(partial_decision.enforceable_minimum, BumpSeverity::Major);
	assert_eq!(partial_decision.completeness, AnalysisCompleteness::Partial);
	assert!(partial_decision.review_required);
}

#[test]
fn changeset_validation_keeps_partial_findings_advisory_unless_strict() {
	let report = report_with_one_breaking_change();

	assert!(changeset_validation_mismatches(&report, false).is_empty());
	assert_eq!(changeset_validation_mismatches(&report, true).len(), 1);
}

fn report_with_one_breaking_change() -> ChangeClassificationReport {
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Signature,
		package_analyses: [(
			"core".to_string(),
			package_with_changes("core", vec![removed_api_change()]),
		)]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};

	classification_report(&analysis, DependencyPropagation::None)
}

fn sample_snapshot_change(
	kind: monochange_snapshot::SnapshotChangeKind,
	path: Vec<String>,
	severity: monochange_snapshot::SnapshotSeverity,
) -> monochange_snapshot::SnapshotChange {
	monochange_snapshot::SnapshotChange {
		severity,
		path,
		summary: format!("sample {kind:?} change"),
		kind,
	}
}

#[test]
fn snapshot_change_kind_names_are_kebab_case_rule_ids() {
	use monochange_snapshot::SnapshotChangeKind;

	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::CommandAdded),
		"command-added"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::CommandRemoved),
		"command-removed"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::CommandDescriptionChanged),
		"command-description-changed"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::OptionAdded),
		"option-added"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::OptionRemoved),
		"option-removed"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::OptionDescriptionChanged),
		"option-description-changed"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::OptionValueWidened),
		"option-value-widened"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::OptionValueNarrowed),
		"option-value-narrowed"
	);

	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::PositionalAdded),
		"positional-added"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::PositionalRemoved),
		"positional-removed"
	);
	assert_eq!(
		snapshot_change_kind_name(&SnapshotChangeKind::PositionalChanged),
		"positional-changed"
	);
}

#[test]
fn cli_surface_findings_use_unique_ids_with_complete_coverage() {
	use monochange_snapshot::SnapshotChangeKind;
	use monochange_snapshot::SnapshotSeverity;

	let report = monochange_snapshot::SnapshotDiffReport {
		recommendation: SnapshotSeverity::Major,
		changes: vec![
			sample_snapshot_change(
				SnapshotChangeKind::OptionRemoved,
				vec!["release".to_string()],
				SnapshotSeverity::Major,
			),
			sample_snapshot_change(
				SnapshotChangeKind::OptionRemoved,
				vec!["release".to_string()],
				SnapshotSeverity::Major,
			),
			sample_snapshot_change(
				SnapshotChangeKind::CommandAdded,
				vec!["migrate".to_string(), "audit".to_string()],
				SnapshotSeverity::Minor,
			),
		],
	};

	let findings = cli_surface_findings(DetectionLevel::Signature, &report);

	assert_eq!(findings.len(), 3);
	assert_eq!(
		findings[0].id,
		"monochange/cli-surface/option-removed/release"
	);
	assert_eq!(
		findings[1].id,
		"monochange/cli-surface/option-removed/release/2"
	);
	assert_eq!(
		findings[2].id,
		"monochange/cli-surface/command-added/migrate/audit"
	);

	for finding in &findings {
		assert_eq!(finding.surface, "cli");
		assert_eq!(finding.analyzer.id, "monochange/cli-surface");
		assert_eq!(finding.confidence, ClassificationConfidence::High);

		assert_eq!(
			finding.coverage.completeness,
			AnalysisCompleteness::Complete
		);
		assert!(finding.comparisons.contains(&ComparisonKind::PullRequest));
	}

	assert_eq!(findings[0].impact, CompatibilityImpact::Breaking);
	assert_eq!(findings[0].bump, BumpSeverity::Major);
	assert_eq!(findings[2].impact, CompatibilityImpact::Additive);
	assert_eq!(findings[2].bump, BumpSeverity::Minor);
	assert_eq!(findings[2].location, PathBuf::from("migrate/audit"));
}

#[test]
fn cli_classification_description_describes_every_status() {
	let diffed = cli_classification_description(&PackageCliClassification {
		name: "monochange".to_string(),
		status: CliSnapshotStatus::Diffed,
		recommendation: Some(BumpSeverity::Major),
		finding_count: Some(2),
		baseline: Some(".monochange/cli-snapshots/monochange.json".to_string()),
	});
	assert_eq!(
		diffed,
		"diffed against `.monochange/cli-snapshots/monochange.json`, recommendation `major`, 2 finding(s)"
	);

	let none_recommendation = cli_classification_description(&PackageCliClassification {
		name: "monochange".to_string(),
		status: CliSnapshotStatus::Diffed,
		recommendation: Some(BumpSeverity::None),
		finding_count: Some(0),
		baseline: Some(".monochange/cli-snapshots/monochange.json".to_string()),
	});
	assert!(none_recommendation.contains("recommendation `none`"));

	for (status, expected) in [
		(CliSnapshotStatus::MissingBaseline, "no committed baseline"),
		(CliSnapshotStatus::StaleBaseline, "stale or unparsable"),
		(CliSnapshotStatus::Failed, "capture failed"),
		(CliSnapshotStatus::Skipped, "capture skipped"),
	] {
		let description = cli_classification_description(&PackageCliClassification {
			name: "monochange".to_string(),
			status,
			recommendation: None,
			finding_count: None,
			baseline: None,
		});
		assert!(description.contains(expected), "{status:?}: {description}");
	}
}

#[test]
fn collect_cli_surface_classification_skips_unchanged_registered_clis() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let package_dir = tempdir.path().join("crates/demo");
	fs::create_dir_all(&package_dir).unwrap_or_else(|error| panic!("create dir: {error}"));
	fs::write(
		package_dir.join("Cargo.toml"),
		"[package]\nname = \"demo\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.demo]\npath = \"crates/demo\"\ntype = \"cargo\"\ncli = { name = \"demo\", snapshot = \"cat cli.json\" }\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	let configuration = monochange_config::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let options = ClassifyOptions::default();
	let mut findings = Vec::new();
	let mut warnings = Vec::new();

	let classification = collect_cli_surface_classification(
		tempdir.path(),
		&configuration,
		"demo",
		&[],
		&options,
		&mut findings,
		&mut warnings,
	);

	assert_eq!(classification, None);
	assert!(findings.is_empty());
	assert!(warnings.is_empty());
}

#[test]
fn skipped_classification_defaults_to_skipping_the_release_label() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("monochange.toml"), "")
		.unwrap_or_else(|error| panic!("write config: {error}"));
	let configuration = monochange_config::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(
		configuration.changesets.classification.skip_labels,
		vec!["release"]
	);

	let options = ClassifyOptions {
		base: Some("HEAD".to_string()),
		labels: vec!["release".to_string()],
		..ClassifyOptions::default()
	};
	let report = skipped_classification(tempdir.path(), &configuration, &options)
		.unwrap_or_else(|error| panic!("skip check: {error}"))
		.unwrap_or_else(|| panic!("the release label should skip by default"));
	assert!(report.skipped);
}

#[test]
fn schema_module_generates_the_classification_report_schema() {
	let schema = crate::schema::classification_report();

	let json = serde_json::to_value(&schema)
		.unwrap_or_else(|error| panic!("classification schema should serialize: {error}"));

	// The `$id` is added by xtask post-processing; the generator itself only
	// produces the schema body.
	assert!(json["$id"].is_null() || json["$id"].as_str().is_none());
	assert!(json["properties"]["schema_version"].is_object());
	assert!(json["properties"]["packages"].is_object());
}

#[test]
fn reports_list_inherited_base_branch_findings_apart_from_the_pull_request() {
	let mut report = report_with_one_breaking_change();

	let markdown = render_markdown_report(&report);
	assert!(markdown.contains("- Base branch: `origin/main`\n"));
	assert!(!markdown.contains("Head commit"));
	assert!(!markdown.contains("not part of this pull request"));
	assert!(render_text_report(&report).contains("Base branch: origin/main\n"));

	report.base_commit = Some("0123456789abcdef0123456789abcdef01234567".to_string());
	report.head_commit = Some("fedcba9876543210fedcba9876543210fedcba98".to_string());
	let package = report
		.packages
		.first_mut()
		.unwrap_or_else(|| panic!("the sample report has one package"));
	let mut inherited = finding_from_semantic_change(
		"inherited".to_string(),
		"cargo/public-api",
		&modified_api_change(),
		DetectionLevel::Signature,
	);
	inherited.comparisons = [ComparisonKind::Release, ComparisonKind::ReleaseToDefault]
		.into_iter()
		.collect();
	package.findings.push(inherited);

	let markdown = render_markdown_report(&report);
	assert!(markdown.contains("- Base branch: `origin/main` (`0123456789ab`)\n"));
	assert!(markdown.contains("- Head commit: `fedcba987654`\n"));
	assert!(markdown.contains("- Findings: 1\n"));
	let inherited_header = markdown
		.find("- Unreleased changes already on `origin/main` (not part of this pull request): 1\n")
		.unwrap_or_else(|| panic!("inherited findings header missing: {markdown}"));
	let current_line = markdown
		.find("- `cargo/public-api/public_api/removed/function/crate::old`:")
		.unwrap_or_else(|| panic!("pull request finding missing: {markdown}"));
	let inherited_line = markdown
		.find("- `inherited`:")
		.unwrap_or_else(|| panic!("inherited finding missing: {markdown}"));
	assert!(
		current_line < inherited_header && inherited_header < inherited_line,
		"pull request findings must precede inherited findings: {markdown}"
	);

	let text = render_text_report(&report);
	assert!(text.contains("Base branch: origin/main (0123456789ab)\n"));
	assert!(text.contains("Head commit: fedcba987654\n"));
	assert!(text.contains("  Findings: 1\n"));
	assert!(text.contains(
		"  Unreleased changes already on origin/main (not part of this pull request): 1\n"
	));
	assert!(!text.contains('`'));
}
