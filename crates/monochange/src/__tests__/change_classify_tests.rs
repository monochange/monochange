use std::ffi::OsString;
use std::path::PathBuf;

use monochange_core::ApiChangeKind;
use monochange_core::ApiConfidence;
use monochange_core::BumpSeverity;
use monochange_core::Ecosystem;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;

use super::*;

#[test]
fn parse_change_classify_options_accepts_agent_workflow_shape() {
	let args = [
		OsString::from("mc"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--base"),
		OsString::from("origin/main"),
		OsString::from("--format"),
		OsString::from("json"),
	];

	let options = parse_change_classify_options(&args)
		.unwrap_or_else(|error| panic!("parse options: {error}"))
		.unwrap_or_else(|| panic!("expected change classify options"));

	assert_eq!(options.base, "origin/main");
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.format, OutputFormat::Json);
}

#[test]
fn parse_api_diff_options_accepts_snapshot_workflow_shape() {
	let args = [
		OsString::from("mc"),
		OsString::from("api"),
		OsString::from("diff"),
		OsString::from("--base"),
		OsString::from("origin/main"),
		OsString::from("--format"),
		OsString::from("json"),
	];

	let options = parse_api_diff_options(&args)
		.unwrap_or_else(|error| panic!("parse options: {error}"))
		.unwrap_or_else(|| panic!("expected api diff options"));

	assert_eq!(options.base, "origin/main");
	assert_eq!(options.format, OutputFormat::Json);
}

#[test]
fn parse_changeset_validate_api_options_accepts_advisory_workflow_shape() {
	let args = [
		OsString::from("mc"),
		OsString::from("changeset"),
		OsString::from("validate"),
		OsString::from("--api"),
		OsString::from("--base"),
		OsString::from("origin/main"),
	];

	let options = parse_changeset_validate_api_options(&args)
		.unwrap_or_else(|error| panic!("parse options: {error}"))
		.unwrap_or_else(|| panic!("expected changeset api options"));

	assert_eq!(options.base, "origin/main");
	assert_eq!(options.head, "HEAD");
	assert_eq!(options.format, OutputFormat::Markdown);
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
	};

	let report = classification_report(&analysis);

	assert_eq!(report.recommendation, BumpSeverity::Major);
	assert_eq!(report.packages.len(), 1);
	let package = report.packages.first().unwrap();
	assert_eq!(package.confidence, "high");
	assert_eq!(package.api_changes.len(), 1);
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
