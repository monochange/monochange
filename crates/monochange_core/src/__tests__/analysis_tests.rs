use super::*;
use crate::BumpSeverity;
use crate::PackageRecord;
use crate::PublishState;

#[test]
fn package_snapshot_file_lookup_finds_matching_paths() {
	let snapshot = PackageSnapshot {
		label: "after".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("src/lib.rs"),
			contents: "pub fn greet() {}".to_string(),
		}],
	};

	let file = snapshot
		.file(Path::new("src/lib.rs"))
		.unwrap_or_else(|| panic!("expected file in snapshot"));
	assert_eq!(file.contents, "pub fn greet() {}");
}

#[test]
fn package_analysis_context_exposes_package_root() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/repo/crates/core/Cargo.toml"),
		PathBuf::from("/repo"),
		None,
		PublishState::Public,
	);
	let context = PackageAnalysisContext {
		repo_root: Path::new("/repo"),
		package: &package,
		detection_level: DetectionLevel::Signature,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: None,
	};

	assert_eq!(context.package_root(), Path::new("/repo/crates/core"));
}

#[test]
fn semantic_change_assessment_serializes_analyzer_provenance_and_fallback() {
	let evidence = SemanticAnalyzerEvidence::new(
		"npm/typescript",
		"typescript",
		SemanticAnalysisCompleteness::Partial,
		"the root export could not emit declarations",
	)
	.with_version("6.0.3")
	.with_fallback_reason("tsconfig.json extends an unavailable file");
	let assessment = SemanticChangeAssessment::new(
		SemanticAnalysisOutcome::Inconclusive,
		BumpSeverity::Patch,
		ApiConfidence::Low,
		evidence,
	);
	let json = serde_json::to_value(&assessment)
		.unwrap_or_else(|error| panic!("serialize semantic assessment: {error}"));

	assert_eq!(json["outcome"], "inconclusive");
	assert_eq!(json["suggestedBump"], "patch");
	assert_eq!(json["evidence"]["analyzerId"], "npm/typescript");
	assert_eq!(json["evidence"]["engine"], "typescript");
	assert_eq!(json["evidence"]["version"], "6.0.3");
	assert_eq!(json["evidence"]["completeness"], "partial");
	assert_eq!(
		json["evidence"]["fallbackReason"],
		"tsconfig.json extends an unavailable file"
	);
}

#[test]
fn cargo_semver_checks_settings_default_to_a_disabled_host_matrix() {
	let settings = CargoSemverChecksSettings::default();

	assert!(!settings.enabled);
	assert_eq!(settings.timeout_seconds, 300);
	assert_eq!(settings.matrix, vec![CargoSemverMatrixCell::default()]);
}

#[test]
fn semantic_analyzer_evidence_serializes_machine_readable_checks() {
	let check = SemanticAnalyzerCheck::new(
		"all-features",
		SemanticAnalyzerCheckStatus::Checked,
		BTreeMap::from([("target".to_string(), "host".to_string())]),
	)
	.with_result(SemanticAnalysisOutcome::Breaking, BumpSeverity::Major)
	.with_diagnostics(vec![
		SemanticAnalyzerDiagnostic::new("struct_missing", "public struct removed")
			.with_reference("https://example.com/struct_missing"),
	]);
	let evidence = SemanticAnalyzerEvidence::new(
		"cargo/cargo-semver-checks",
		"cargo-semver-checks",
		SemanticAnalysisCompleteness::Complete,
		"1/1 matrix cells checked",
	)
	.with_checks(vec![check]);
	let json = serde_json::to_value(evidence)
		.unwrap_or_else(|error| panic!("evidence should serialize: {error}"));

	assert_eq!(json["checks"][0]["name"], "all-features");
	assert_eq!(json["checks"][0]["suggestedBump"], "major");
	assert_eq!(
		json["checks"][0]["diagnostics"][0]["code"],
		"struct_missing"
	);
}

#[test]
fn semantic_change_builder_attaches_optional_evidence() {
	let assessment = SemanticChangeAssessment::new(
		SemanticAnalysisOutcome::Breaking,
		BumpSeverity::Major,
		ApiConfidence::High,
		SemanticAnalyzerEvidence::new(
			"npm/typescript",
			"typescript",
			SemanticAnalysisCompleteness::Complete,
			"all explicit exports",
		),
	);
	let change = SemanticChange::new(
		SemanticChangeCategory::PublicApi,
		SemanticChangeKind::Modified,
		"function",
		".#parse",
		"changed parse",
		"dist/index.d.ts",
	)
	.with_before_signature("parse(value: string): string")
	.with_after_signature("parse(value: number): string")
	.with_assessment(assessment);

	assert_eq!(
		change.before_signature.as_deref(),
		Some("parse(value: string): string")
	);
	assert_eq!(
		change.after_signature.as_deref(),
		Some("parse(value: number): string")
	);
	assert_eq!(
		change.assessment.map(|value| value.suggested_bump),
		Some(BumpSeverity::Major)
	);
}

#[test]
fn api_snapshot_sorts_items_for_stable_json_output() {
	let snapshot = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new("function", "crate::z", Some("pub fn z()".to_string())),
			ApiItem::new("function", "crate::a", Some("pub fn a()".to_string())),
		],
		Vec::new(),
	);

	let json = serde_json::to_string_pretty(&snapshot)
		.unwrap_or_else(|error| panic!("serialize snapshot: {error}"));

	assert!(json.contains("\"schemaVersion\": 1"));
	assert!(
		json.find("function:crate::a") < json.find("function:crate::z"),
		"items should be sorted by stable id: {json}"
	);
}

#[test]
fn api_snapshot_diff_classifies_removed_added_and_modified_items() {
	let before = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::removed",
				Some("pub fn removed()".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed()".to_string()),
			),
		],
		Vec::new(),
	);
	let after = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed(value: u8)".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::added",
				Some("pub fn added()".to_string()),
			),
		],
		Vec::new(),
	);

	let diff = before.diff(&after);

	assert_eq!(diff.suggested_bump, BumpSeverity::Major);
	assert_eq!(diff.changes.len(), 3);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.kind == ApiChangeKind::Removed)
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.kind == ApiChangeKind::Added)
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.kind == ApiChangeKind::Modified)
	);
}

#[test]
fn diff_api_snapshots_reports_added_removed_modified_and_warnings() {
	let before = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::removed",
				Some("pub fn removed()".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed()".to_string()),
			),
			ApiItem::new("function", "crate::same", Some("pub fn same()".to_string())),
		],
		vec!["before warning".to_string()],
	);
	let after = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![
			ApiItem::new(
				"function",
				"crate::added",
				Some("pub fn added()".to_string()),
			),
			ApiItem::new(
				"function",
				"crate::changed",
				Some("pub fn changed(value: u8)".to_string()),
			),
			ApiItem::new("function", "crate::same", Some("pub fn same()".to_string())),
		],
		vec!["after warning".to_string()],
	);

	let diff = diff_api_snapshots(&before, &after);

	assert_eq!(diff.suggested_bump, BumpSeverity::Major);
	assert_eq!(diff.changes.len(), 3);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.summary.contains("added"))
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.summary.contains("removed"))
	);
	assert!(
		diff.changes
			.iter()
			.any(|change| change.summary.contains("changed"))
	);
	assert_eq!(diff.warnings, vec!["before warning", "after warning"]);
}

#[test]
fn api_diff_is_empty_when_no_changes_are_present() {
	let before = ApiSnapshot::new(
		"core",
		"core",
		Ecosystem::Cargo,
		"cargo/public-api",
		vec![ApiItem::new(
			"function",
			"crate::same",
			Some("pub fn same()".to_string()),
		)],
		Vec::new(),
	);
	let after = before.clone();

	let diff = diff_api_snapshots(&before, &after);

	assert!(diff.is_empty());
	assert_eq!(diff.suggested_bump, BumpSeverity::None);
}

#[test]
fn package_path_matcher_applies_root_additional_and_ignored_paths() {
	let matcher = PackagePathMatcher::new(
		"core",
		Path::new("packages/core"),
		&["schema/**".to_string()],
		&["tests/**".to_string()],
	);

	assert_eq!(matcher.package_id(), "core");
	assert_eq!(
		matcher.classify(Path::new("packages/core/src/lib.rs")),
		PackagePathMatch::Touched
	);
	assert_eq!(
		matcher.classify(Path::new("packages/core/tests/api.rs")),
		PackagePathMatch::Ignored
	);
	assert_eq!(
		matcher.classify(Path::new("schema/public.json")),
		PackagePathMatch::Touched
	);
	assert_eq!(
		matcher.classify(Path::new("packages/other/src/lib.rs")),
		PackagePathMatch::Unmatched
	);

	let workspace_matcher = PackagePathMatcher::new(
		"workspace",
		Path::new(""),
		&[],
		&["fixtures/**".to_string()],
	);
	assert_eq!(
		workspace_matcher.classify(Path::new("fixtures/tests/example.ts")),
		PackagePathMatch::Ignored
	);
	assert_eq!(
		workspace_matcher.classify(Path::new("README.md")),
		PackagePathMatch::Touched
	);
}
