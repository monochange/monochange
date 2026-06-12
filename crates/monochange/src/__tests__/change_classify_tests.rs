use std::ffi::OsString;
use std::path::PathBuf;

use monochange_core::ApiChangeKind;
use monochange_core::ApiConfidence;
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
fn parse_change_classify_options_accepts_agent_workflow_shape() {
	let args = [
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--base"),
		OsString::from("origin/main"),
		OsString::from("--format"),
		OsString::from("json"),
		OsString::from("--dependency-propagation"),
		OsString::from("public"),
	];

	let options = parse_change_classify_options(&args)
		.unwrap_or_else(|error| panic!("parse options: {error}"))
		.unwrap_or_else(|| panic!("expected change classify options"));

	assert_eq!(options.base, "origin/main");
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.format, OutputFormat::Json);
	assert_eq!(
		options.dependency_propagation,
		DependencyPropagation::Public
	);
}

#[test]
fn parse_api_diff_options_accepts_snapshot_workflow_shape() {
	let args = [
		OsString::from("monochange"),
		OsString::from("api"),
		OsString::from("diff"),
		OsString::from("--base"),
		OsString::from("origin/main"),
		OsString::from("--format"),
		OsString::from("json"),
		OsString::from("--dependency-propagation"),
		OsString::from("public"),
	];

	let options = parse_api_diff_options(&args)
		.unwrap_or_else(|error| panic!("parse options: {error}"))
		.unwrap_or_else(|| panic!("expected api diff options"));

	assert_eq!(options.base, "origin/main");
	assert_eq!(options.format, OutputFormat::Json);
	assert_eq!(
		options.dependency_propagation,
		DependencyPropagation::Public
	);
}

#[test]
fn parse_changeset_validate_api_options_accepts_advisory_workflow_shape() {
	let args = [
		OsString::from("monochange"),
		OsString::from("changeset"),
		OsString::from("validate"),
		OsString::from("--api"),
		OsString::from("--base"),
		OsString::from("origin/main"),
		OsString::from("--dependency-propagation"),
		OsString::from("public"),
	];

	let options = parse_changeset_validate_api_options(&args)
		.unwrap_or_else(|error| panic!("parse options: {error}"))
		.unwrap_or_else(|| panic!("expected changeset api options"));

	assert_eq!(options.base, "origin/main");
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.format, OutputFormat::Markdown);
	assert_eq!(
		options.dependency_propagation,
		DependencyPropagation::Public
	);
}

#[test]
fn parse_dependency_propagation_rejects_unknown_modes() {
	let args = [
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--dependency-propagation"),
		OsString::from("transitive"),
	];

	let error = parse_change_classify_options(&args).expect_err("expected parse error");

	assert!(error.render().contains("transitive"));
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
	assert_eq!(package.confidence, "high");
	assert_eq!(package.api_changes.len(), 1);
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
fn markdown_report_is_agent_readable() {
	let report = ChangeClassificationReport {
		frame: "origin/main...HEAD".to_string(),
		recommendation: BumpSeverity::Minor,
		packages: vec![PackageClassification {
			package_id: "ui".to_string(),
			package_name: "ui".to_string(),
			ecosystem: Ecosystem::Npm,
			recommendation: BumpSeverity::Minor,
			confidence: "high".to_string(),
			summary: "additive public API impact inferred from 1 semantic change(s)".to_string(),
			analyzer_id: Some("npm/public-api".to_string()),
			api_changes: vec![api_change_from_semantic_change(&added_export_change())],
			semantic_changes: vec![added_export_change()],
			warnings: Vec::new(),
		}],
		warnings: Vec::new(),
	};

	let markdown = render_markdown_report(&report);

	assert!(markdown.contains("# API change classification"));
	assert!(markdown.contains("Recommended bump: `minor`"));
	assert!(markdown.contains("### `ui`"));
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
fn api_change_mapping_handles_modified_patch_and_low_confidence_changes() {
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

	let dependency_change = api_change_from_semantic_change(&modified);
	let metadata_change = api_change_from_semantic_change(&unchanged);

	assert_eq!(dependency_change.kind, ApiChangeKind::Modified);
	assert_eq!(dependency_change.confidence, ApiConfidence::Medium);
	assert_eq!(metadata_change.confidence, ApiConfidence::Medium);
}
