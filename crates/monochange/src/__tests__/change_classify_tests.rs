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
fn classify_options_defaults_are_safe_for_agent_use() {
	let options = ClassifyOptions::default();

	assert_eq!(options.base, None);
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.release, None);
	assert!(options.packages.is_empty());
	assert_eq!(options.detection_level, DetectionLevel::Signature);
	assert!(!options.include_unchanged);
	assert!(!options.strict);
	assert_eq!(options.format, OutputFormat::Text);
	assert_eq!(options.output, None);
	assert_eq!(options.dependency_propagation, DependencyPropagation::None);
}

#[test]
fn classify_options_cover_all_supported_formats_and_detection_levels() {
	for (format, expected) in [
		("markdown", OutputFormat::Markdown),
		("md", OutputFormat::Markdown),
		("json", OutputFormat::Json),
		("json-min", OutputFormat::JsonMin),
		("text", OutputFormat::Text),
	] {
		let matches = crate::cli::build_command_with_cli("monochange", &[])
			.try_get_matches_from(["monochange", "change", "classify", "--format", format])
			.unwrap_or_else(|error| panic!("parse {format}: {error}"));
		let (_, change_matches) = matches.subcommand().unwrap();
		let (_, classify_matches) = change_matches.subcommand().unwrap();
		let options = classify_options_from_matches(classify_matches)
			.unwrap_or_else(|error| panic!("extract {format}: {error}"));
		assert_eq!(options.format, expected);
	}

	assert_eq!(
		parse_detection_level("basic").unwrap(),
		DetectionLevel::Basic
	);
	assert_eq!(
		parse_detection_level("semantic").unwrap(),
		DetectionLevel::Semantic
	);
	assert!(parse_detection_level("impossible").is_err());
	assert_eq!(
		parse_dependency_propagation("none").unwrap(),
		DependencyPropagation::None
	);
	assert_eq!(
		parse_dependency_propagation("public").unwrap(),
		DependencyPropagation::Public
	);
	assert!(parse_dependency_propagation("transitive").is_err());
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
		compatibility_impact_name(CompatibilityImpact::Unknown),
		"unknown"
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
	assert_eq!(options.format, OutputFormat::Text);
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
		members: vec!["core".to_string()],
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
	assert_eq!(changeset_action(&patch, &[]), ChangesetAction::Create);
	let mut major = patch;
	major.proposed_changeset_bump = BumpSeverity::Major;
	assert_eq!(changeset_action(&major, &existing), ChangesetAction::Update);
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
	assert_eq!(findings[0].impact, CompatibilityImpact::Unknown);
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
		CompatibilityImpact::Unknown
	);

	let patch = build_recommendation(&findings, true, false);
	assert!(recommendation_summary(&patch, &findings).contains("unclassified"));
	let mut compatible_findings = findings.clone();
	compatible_findings[0].impact = CompatibilityImpact::Compatible;
	let compatible_patch = build_recommendation(&compatible_findings, true, false);
	assert!(recommendation_summary(&compatible_patch, &compatible_findings).contains("compatible"));

	let no_change = no_change_recommendation();
	assert_eq!(
		recommendation_summary(&no_change, &[]),
		"no package change requires a changeset"
	);
	let mut review = no_change;
	review.review_required = true;
	assert!(recommendation_summary(&review, &[]).contains("requires review"));

	assert!(analyzer_coverage_note("npm/exports").contains("TypeScript assignability"));
	assert!(analyzer_coverage_note("deno/exports").contains("TypeScript assignability"));
	assert!(analyzer_coverage_note("dart/public-api").contains("Dart declaration"));
	assert!(analyzer_coverage_note("custom/analyzer").contains("does not declare"));
}

#[test]
fn release_owner_reports_package_and_group_identity() {
	let package = EffectiveReleaseIdentity {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		group_id: None,
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		members: vec!["core".to_string()],
	};
	let group = EffectiveReleaseIdentity {
		owner_id: "workspace".to_string(),
		owner_kind: ReleaseOwnerKind::Group,
		group_id: Some("workspace".to_string()),
		..package.clone()
	};

	assert_eq!(release_owner(None, None), None);
	assert_eq!(
		release_owner(Some(&package), Some("core/v1.0.0".to_string()))
			.unwrap()
			.kind,
		"package"
	);
	assert_eq!(release_owner(Some(&group), None).unwrap().kind, "group");
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
	assert!(markdown.contains("Evidence: cargo/public-api 1; coverage partial"));
}

#[test]
fn markdown_report_surfaces_semantic_engine_and_fallback_evidence() {
	let mut change = removed_api_change();
	change.assessment = Some(monochange_core::SemanticChangeAssessment::new(
		monochange_core::SemanticAnalysisOutcome::Inconclusive,
		BumpSeverity::Patch,
		monochange_core::ApiConfidence::Low,
		monochange_core::SemanticAnalyzerEvidence::new(
			"npm/typescript",
			"typescript",
			monochange_core::SemanticAnalysisCompleteness::Partial,
			"syntax fallback",
		)
		.with_version("6.0.3")
		.with_fallback_reason("tsconfig could not be resolved"),
	));
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::CustomRange {
			base: "origin/main".to_string(),
			head: "HEAD".to_string(),
		},
		detection_level: monochange_analysis::DetectionLevel::Semantic,
		package_analyses: [("ui".to_string(), package_with_changes("ui", vec![change]))]
			.into_iter()
			.collect(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};
	let report = classification_report(&analysis, DependencyPropagation::None);

	let markdown = render_markdown_report(&report);

	assert!(markdown.contains(
		"Evidence: npm/typescript via typescript 6.0.3; coverage partial: syntax fallback; fallback: tsconfig could not be resolved"
	));
}

#[test]
fn text_report_contains_no_markdown_headings_or_code_spans() {
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
	let mut report = classification_report(&analysis, DependencyPropagation::None);
	report
		.warnings
		.push("Review the generated change.".to_string());

	let text = render_text_report(&report);

	assert!(text.starts_with("Change classification\n"));
	assert!(text.contains("Recommended bump: minor"));
	assert!(text.contains("\nui\n"));
	assert!(text.contains("\nWarnings\n"));
	assert!(!text.contains('#'));
	assert!(!text.contains('`'));
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
fn semantic_finding_mapping_keeps_impact_and_evidence_separate() {
	let modified = SemanticChange::new(
		SemanticChangeCategory::Dependency,
		SemanticChangeKind::Modified,
		"dependency",
		"serde",
		"changed dependency `serde`",
		PathBuf::from("Cargo.toml"),
	)
	.with_before_signature("serde = 1")
	.with_after_signature("serde = 2");
	let unchanged = SemanticChange::new(
		SemanticChangeCategory::Metadata,
		SemanticChangeKind::Modified,
		"implementation",
		"crate::detail",
		"changed internal implementation",
		PathBuf::from("src/lib.rs"),
	);

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
fn semantic_finding_uses_explicit_analyzer_assessment() {
	let mut change = removed_api_change();
	change.kind = SemanticChangeKind::Modified;
	change.summary = "implementation changed without changing declarations".to_string();
	change.assessment = Some(monochange_core::SemanticChangeAssessment::new(
		monochange_core::SemanticAnalysisOutcome::Compatible,
		BumpSeverity::None,
		monochange_core::ApiConfidence::High,
		monochange_core::SemanticAnalyzerEvidence::new(
			"npm/typescript",
			"typescript",
			monochange_core::SemanticAnalysisCompleteness::Complete,
			"all explicit typed exports were checked",
		)
		.with_version("6.0.3"),
	));

	let finding = finding_from_semantic_change(
		finding_id("npm/package-json", &change),
		"npm/package-json",
		&change,
		monochange_analysis::DetectionLevel::Semantic,
	);

	assert_eq!(finding.impact, CompatibilityImpact::Compatible);
	assert_eq!(finding.bump, BumpSeverity::None);
	assert_eq!(finding.confidence, ClassificationConfidence::High);
	assert_eq!(finding.analyzer.id, "npm/typescript");
	assert_eq!(finding.analyzer.engine.as_deref(), Some("typescript"));
	assert_eq!(finding.analyzer.version, "6.0.3");
	assert_eq!(
		finding.coverage.completeness,
		AnalysisCompleteness::Complete
	);
	assert_eq!(
		finding.coverage.note,
		"all explicit typed exports were checked"
	);
	assert_eq!(finding.coverage.fallback_reason, None);
	let mut current = finding;
	current.comparisons.insert(ComparisonKind::PullRequest);
	let decision = build_recommendation(&[current], true, false);
	assert_eq!(decision.proposed_changeset_bump, BumpSeverity::None);
	assert_eq!(decision.completeness, AnalysisCompleteness::Complete);
	assert!(!decision.review_required);
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
		CompatibilityImpact::Unknown
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
	let decision = build_recommendation(&[finding.clone()], true, false);
	assert_eq!(decision.enforceable_minimum, BumpSeverity::Major);
	assert_eq!(decision.completeness, AnalysisCompleteness::Complete);
	assert!(!decision.review_required);

	finding.coverage.completeness = AnalysisCompleteness::Partial;
	let partial_decision = build_recommendation(&[finding], true, false);
	assert_eq!(partial_decision.enforceable_minimum, BumpSeverity::Major);
	assert_eq!(partial_decision.completeness, AnalysisCompleteness::Partial);
	assert!(partial_decision.review_required);
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
