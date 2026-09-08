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

use super::*;

#[test]
fn classify_options_from_matches_accepts_agent_workflow_shape() {
	let matches = crate::cli::build_command_with_cli("monochange", &[])
		.try_get_matches_from([
			"monochange",
			"--jq",
			".packages",
			"change",
			"classify",
			"--base",
			"origin/main",
			"--format=json",
			"--dependency-propagation",
			"public",
		])
		.unwrap_or_else(|error| panic!("parse command: {error}"));
	let (_, change_matches) = matches.subcommand().unwrap();
	let (_, classify_matches) = change_matches.subcommand().unwrap();
	let options = classify_options_from_matches(classify_matches)
		.unwrap_or_else(|error| panic!("extract options: {error}"));

	assert_eq!(options.base, Some("origin/main".to_string()));
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.format, OutputFormat::Json);
	assert_eq!(
		options.dependency_propagation,
		DependencyPropagation::Public
	);
}

#[test]
fn classify_options_from_matches_accepts_api_diff_shape() {
	let matches = crate::cli::build_command_with_cli("monochange", &[])
		.try_get_matches_from([
			"monochange",
			"api",
			"diff",
			"--base",
			"origin/main",
			"--format",
			"json",
			"--dependency-propagation",
			"public",
		])
		.unwrap_or_else(|error| panic!("parse command: {error}"));
	let (_, api_matches) = matches.subcommand().unwrap();
	let (_, diff_matches) = api_matches.subcommand().unwrap();
	let options = classify_options_from_matches(diff_matches)
		.unwrap_or_else(|error| panic!("extract options: {error}"));

	assert_eq!(options.base, Some("origin/main".to_string()));
	assert_eq!(options.format, OutputFormat::Json);
	assert_eq!(
		options.dependency_propagation,
		DependencyPropagation::Public
	);
}

#[test]
fn classify_options_from_matches_accepts_changeset_validation_shape() {
	let matches = crate::cli::build_command_with_cli("monochange", &[])
		.try_get_matches_from([
			"monochange",
			"changeset",
			"validate",
			"--api",
			"--strict",
			"--base",
			"origin/main",
			"--dependency-propagation",
			"public",
		])
		.unwrap_or_else(|error| panic!("parse command: {error}"));
	let (_, changeset_matches) = matches.subcommand().unwrap();
	let (_, validate_matches) = changeset_matches.subcommand().unwrap();
	let options = classify_options_from_matches(validate_matches)
		.unwrap_or_else(|error| panic!("extract options: {error}"));

	assert_eq!(options.base, Some("origin/main".to_string()));
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.format, OutputFormat::Markdown);
	assert!(options.strict);
	assert_eq!(
		options.dependency_propagation,
		DependencyPropagation::Public
	);
}

#[test]
fn clap_rejects_unknown_dependency_propagation_modes() {
	let error = crate::cli::build_command_with_cli("monochange", &[])
		.try_get_matches_from([
			"monochange",
			"change",
			"classify",
			"--dependency-propagation",
			"transitive",
		])
		.expect_err("expected parse error");

	assert!(error.to_string().contains("transitive"));
}

#[test]
fn release_tag_version_supports_primary_namespaced_and_custom_formats() {
	assert_eq!(
		release_tag_version("v1.2.3", &VersionFormat::Primary, "core", "cargo"),
		Some(semver::Version::new(1, 2, 3))
	);
	assert_eq!(
		release_tag_version(
			"core/v2.0.0-beta.1",
			&VersionFormat::Namespaced,
			"core",
			"cargo",
		),
		Some(semver::Version::parse("2.0.0-beta.1").unwrap())
	);
	assert_eq!(
		release_tag_version(
			"release/cargo/core/3.4.5",
			&VersionFormat::Custom("release/{{ ecosystem }}/{{ name }}/{{ version }}".to_string(),),
			"core",
			"cargo",
		),
		Some(semver::Version::new(3, 4, 5))
	);
	assert!(
		release_tag_version("other/v1.2.3", &VersionFormat::Namespaced, "core", "cargo").is_none()
	);
}

#[test]
fn classification_report_recommends_highest_api_impact() {
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

	let report = classification_report(&analysis, DependencyPropagation::None);

	assert_eq!(report.recommendation, BumpSeverity::Major);
	assert_eq!(report.packages.len(), 1);
	let package = report.packages.first().unwrap();
	assert_eq!(
		package.decision.confidence,
		ClassificationConfidence::Medium
	);
	assert_eq!(package.findings.len(), 1);
	assert!(package.decision.review_required);
}

#[test]
fn working_tree_evidence_does_not_inflate_the_net_pull_request_bump() {
	let working = ChangeAnalysis {
		frame: ChangeFrame::WorkingDirectory,
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
	let evidence = [PackageEvidence {
		kind: ComparisonKind::WorkingTree,
		analysis: &working,
	}];

	let findings = collect_findings("core", &evidence, DetectionLevel::Signature);
	let decision = build_recommendation(&findings, false, false);

	assert_eq!(decision.proposed_changeset_bump, BumpSeverity::None);
	assert_eq!(
		decision.compatibility_impact,
		CompatibilityImpact::Compatible
	);
	assert_eq!(
		findings[0].comparisons,
		[ComparisonKind::WorkingTree].into_iter().collect()
	);
}

#[test]
fn findings_keep_distinct_signatures_for_each_comparison() {
	let mut pull_request_change = removed_api_change();
	pull_request_change.kind = SemanticChangeKind::Modified;
	pull_request_change.after_signature = Some("pub fn old(value: &str)".to_string());
	let mut release_change = pull_request_change.clone();
	release_change.before_signature = Some("pub fn old(value: String)".to_string());
	let pull_request = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: DetectionLevel::Signature,
		package_analyses: [(
			"core".to_string(),
			package_with_changes("core", vec![pull_request_change]),
		)]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};
	let release = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "v1.0.0".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: DetectionLevel::Signature,
		package_analyses: [(
			"core".to_string(),
			package_with_changes("core", vec![release_change]),
		)]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};
	let evidence = [
		PackageEvidence {
			kind: ComparisonKind::PullRequest,
			analysis: &pull_request,
		},
		PackageEvidence {
			kind: ComparisonKind::SourceDelta,
			analysis: &release,
		},
		PackageEvidence {
			kind: ComparisonKind::Release,
			analysis: &release,
		},
	];

	let findings = collect_findings("core", &evidence, DetectionLevel::Signature);

	assert_eq!(findings.len(), 2);
	assert!(findings.iter().any(|finding| {
		finding.comparisons == [ComparisonKind::PullRequest].into_iter().collect()
			&& finding.before.as_deref() == Some("pub fn old()")
			&& finding.id.contains('@')
	}));
	assert!(findings.iter().any(|finding| {
		finding.comparisons
			== [ComparisonKind::SourceDelta, ComparisonKind::Release]
				.into_iter()
				.collect()
			&& finding.before.as_deref() == Some("pub fn old(value: String)")
			&& finding.id.contains('@')
	}));
}

#[test]
fn release_floor_never_falls_below_the_net_pull_request_bump() {
	let mut current = finding_from_semantic_change(
		"current".to_string(),
		"cargo/public-api",
		&removed_api_change(),
		DetectionLevel::Signature,
	);
	current.comparisons.insert(ComparisonKind::PullRequest);
	let mut release = finding_from_semantic_change(
		"release".to_string(),
		"cargo/public-api",
		&patch_dependency_change(),
		DetectionLevel::Signature,
	);
	release.comparisons.insert(ComparisonKind::Release);

	let decision = build_recommendation(&[current, release], true, true);

	assert_eq!(decision.proposed_changeset_bump, BumpSeverity::Major);
	assert_eq!(decision.release_floor, BumpSeverity::Major);
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
fn public_dependency_propagation_returns_early_without_package_records() {
	let core = package_with_changes("core", vec![added_export_change()]);
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Signature,
		package_analyses: [("core".to_string(), core)].into_iter().collect(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};
	let mut packages = Vec::new();
	let mut recommendation = BumpSeverity::None;

	propagate_public_dependency_impacts(&analysis, &mut packages, &mut recommendation);

	assert!(packages.is_empty());
	assert_eq!(recommendation, BumpSeverity::None);
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
fn public_dependency_propagation_can_set_patch_recommendation_when_called_standalone() {
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
	let mut packages = Vec::new();
	let mut recommendation = BumpSeverity::None;

	propagate_public_dependency_impacts(&analysis, &mut packages, &mut recommendation);

	assert_eq!(recommendation, BumpSeverity::Patch);
	assert_eq!(packages.len(), 1);
	assert_eq!(packages[0].package_id, app_id);
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
fn changeset_action_reviews_unmatched_intent_and_keeps_matching_intent() {
	let no_change = no_change_recommendation();
	let existing = [ExistingChangeset {
		path: PathBuf::from(".changeset/example.md"),
		bump: Some(BumpSeverity::Patch),
		change_type: Some("patch".to_string()),
	}];

	assert_eq!(
		changeset_action(&no_change, &[]),
		ChangesetAction::NoChangeset
	);
	assert_eq!(
		changeset_action(&no_change, &existing),
		ChangesetAction::Review
	);
	let mut patch = no_change;
	patch.proposed_changeset_bump = BumpSeverity::Patch;
	assert_eq!(changeset_action(&patch, &existing), ChangesetAction::Keep);
}

#[test]
fn markdown_report_is_agent_readable() {
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Signature,
		package_analyses: [(
			"ui".to_string(),
			package_with_changes("ui", vec![added_export_change()]),
		)]
		.into_iter()
		.collect(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};
	let report = classification_report(&analysis, DependencyPropagation::None);

	let markdown = render_markdown_report(&report);

	assert!(markdown.contains("# Change classification"));
	assert!(markdown.contains("Recommended bump: `minor`"));
	assert!(markdown.contains("### `ui`"));
	assert!(markdown.contains("Review required: `true`"));
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
		proposed_changeset_bump: BumpSeverity::None,
		enforceable_minimum: BumpSeverity::None,
		release_floor: BumpSeverity::None,
		confidence: ClassificationConfidence::High,
		completeness: AnalysisCompleteness::Complete,
		review_required: false,
		finding_ids: Vec::new(),
	}
}

fn removed_api_change() -> SemanticChange {
	SemanticChange {
		category: SemanticChangeCategory::PublicApi,
		kind: SemanticChangeKind::Removed,
		item_kind: "function".to_string(),
		item_path: "crate::old".to_string(),
		summary: "removed public function `crate::old`".to_string(),
		file_path: PathBuf::from("src/lib.rs"),
		before_signature: Some("pub fn old()".to_string()),
		after_signature: None,
	}
}

fn added_export_change() -> SemanticChange {
	SemanticChange {
		category: SemanticChangeCategory::Export,
		kind: SemanticChangeKind::Added,
		item_kind: "function".to_string(),
		item_path: "render".to_string(),
		summary: "added export `render`".to_string(),
		file_path: PathBuf::from("src/index.ts"),
		before_signature: None,
		after_signature: Some("export function render()".to_string()),
	}
}

fn patch_dependency_change() -> SemanticChange {
	SemanticChange {
		category: SemanticChangeCategory::Dependency,
		kind: SemanticChangeKind::Modified,
		item_kind: "dependency".to_string(),
		item_path: "serde".to_string(),
		summary: "changed dependency `serde`".to_string(),
		file_path: PathBuf::from("Cargo.toml"),
		before_signature: Some("serde = 1".to_string()),
		after_signature: Some("serde = 1.0.1".to_string()),
	}
}

#[test]
fn semantic_finding_mapping_keeps_impact_and_evidence_separate() {
	let modified = SemanticChange {
		category: SemanticChangeCategory::Dependency,
		kind: SemanticChangeKind::Modified,
		item_kind: "dependency".to_string(),
		item_path: "serde".to_string(),
		summary: "changed dependency `serde`".to_string(),
		file_path: PathBuf::from("Cargo.toml"),
		before_signature: Some("serde = 1".to_string()),
		after_signature: Some("serde = 2".to_string()),
	};
	let unchanged = SemanticChange {
		category: SemanticChangeCategory::Metadata,
		kind: SemanticChangeKind::Modified,
		item_kind: "implementation".to_string(),
		item_path: "crate::detail".to_string(),
		summary: "changed internal implementation".to_string(),
		file_path: PathBuf::from("src/lib.rs"),
		before_signature: None,
		after_signature: None,
	};

	let dependency_finding = finding_from_semantic_change(
		finding_id("cargo/public-api", &modified),
		"cargo/public-api",
		&modified,
		monochange_analysis::DetectionLevel::Signature,
	);
	let metadata_finding = finding_from_semantic_change(
		finding_id("cargo/public-api", &unchanged),
		"cargo/public-api",
		&unchanged,
		monochange_analysis::DetectionLevel::Signature,
	);

	assert_eq!(dependency_finding.change, "modified");
	assert_eq!(dependency_finding.bump, BumpSeverity::Patch);
	assert_eq!(
		dependency_finding.confidence,
		ClassificationConfidence::Medium
	);
	assert_eq!(metadata_finding.impact, CompatibilityImpact::Compatible);
	assert_eq!(
		metadata_finding.confidence,
		ClassificationConfidence::Medium
	);
}

#[test]
fn changeset_validation_enforces_only_high_confidence_findings_by_default() {
	let mut report = report_with_one_breaking_change();
	let package = report.packages.first_mut().unwrap();
	package.decision.enforceable_minimum = BumpSeverity::Major;
	package.existing_changesets.push(ExistingChangeset {
		path: PathBuf::from(".changeset/breaking.md"),
		bump: Some(BumpSeverity::Minor),
		change_type: Some("minor".to_string()),
	});

	let mismatches = changeset_validation_mismatches(&report, false);

	assert_eq!(mismatches.len(), 1);
	assert!(mismatches[0].contains("requires at least `major`"));
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
