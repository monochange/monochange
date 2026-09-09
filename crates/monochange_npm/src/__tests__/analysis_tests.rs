use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::FileChangeKind;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::SemanticAnalysisOutcome;
use rstest::rstest;
use walkdir::WalkDir;

use super::*;

#[rstest]
#[case("equivalent", SemanticAnalysisOutcome::Compatible, BumpSeverity::None)]
#[case("additive", SemanticAnalysisOutcome::Additive, BumpSeverity::Minor)]
#[case("breaking", SemanticAnalysisOutcome::Breaking, BumpSeverity::Major)]
#[case("invalid", SemanticAnalysisOutcome::Inconclusive, BumpSeverity::Patch)]
fn semantic_analysis_uses_typescript_declarations(
	#[case] scenario: &str,
	#[case] expected_outcome: SemanticAnalysisOutcome,
	#[case] expected_bump: BumpSeverity,
) {
	let result = analyze_typescript_fixture(scenario);
	let assessments = result
		.semantic_changes
		.iter()
		.filter_map(|change| change.assessment.as_ref())
		.collect::<Vec<_>>();

	assert!(!assessments.is_empty(), "expected TypeScript evidence");
	assert!(assessments.iter().any(|assessment| {
		assessment.outcome == expected_outcome
			&& assessment.suggested_bump == expected_bump
			&& assessment.evidence.analyzer_id == "npm/typescript"
			&& assessment.evidence.engine == "typescript"
			&& assessment.evidence.version.is_some()
	}));
}

#[test]
fn semantic_analysis_classifies_advanced_typescript_contracts() {
	let result = analyze_typescript_fixture("advanced");
	let outcome = |name: &str| {
		result
			.semantic_changes
			.iter()
			.find(|change| change.item_path == format!(".#{name}"))
			.and_then(|change| change.assessment.as_ref())
			.map(|assessment| assessment.outcome)
	};

	assert_eq!(outcome("parse"), Some(SemanticAnalysisOutcome::Additive));
	assert_eq!(outcome("choose"), Some(SemanticAnalysisOutcome::Compatible));
	assert_eq!(
		outcome("configure"),
		Some(SemanticAnalysisOutcome::Additive)
	);
	assert_eq!(outcome("Options"), Some(SemanticAnalysisOutcome::Additive));
	assert_eq!(outcome("Mutable"), Some(SemanticAnalysisOutcome::Breaking));
	assert_eq!(outcome("Locked"), Some(SemanticAnalysisOutcome::Additive));
	assert_eq!(outcome("Scalar"), Some(SemanticAnalysisOutcome::Breaking));
	assert_eq!(outcome("Mode"), Some(SemanticAnalysisOutcome::Breaking));
	assert_eq!(outcome("Client"), Some(SemanticAnalysisOutcome::Breaking));
	assert_eq!(outcome("Box"), Some(SemanticAnalysisOutcome::Inconclusive));
	assert_eq!(outcome("format"), Some(SemanticAnalysisOutcome::Breaking));
}

#[test]
fn semantic_analysis_keeps_javascript_on_the_syntax_analyzer() {
	let result = analyze_typescript_fixture("javascript");

	assert!(!result.semantic_changes.is_empty());
	assert!(
		result
			.semantic_changes
			.iter()
			.all(|change| change.assessment.is_none())
	);
	assert!(result.warnings.is_empty());
}

#[test]
fn semantic_analysis_compares_declaration_only_packages_without_a_tsconfig() {
	let result = analyze_typescript_fixture("declaration-only");
	let added = result
		.semantic_changes
		.iter()
		.find(|change| change.item_path == ".#format")
		.expect("added declaration export");

	assert_eq!(added.file_path, Path::new("index.d.ts"));
	assert_eq!(
		added.assessment.as_ref().map(|value| value.outcome),
		Some(SemanticAnalysisOutcome::Additive)
	);
	assert!(result.warnings.is_empty());
}

#[test]
fn semantic_analysis_classifies_the_only_typed_entrypoint_removal_as_breaking() {
	let result = analyze_typescript_fixture("entrypoint-removed");
	let removed = result
		.semantic_changes
		.iter()
		.find(|change| change.item_path == ".")
		.expect("removed typed entrypoint");

	assert_eq!(removed.kind, SemanticChangeKind::Removed);
	assert_eq!(
		removed.assessment.as_ref().map(|value| value.outcome),
		Some(SemanticAnalysisOutcome::Breaking)
	);
	assert!(result.warnings.is_empty());
}

#[test]
fn semantic_analysis_classifies_optional_member_removal_as_breaking() {
	let result = analyze_typescript_fixture("member-removed");
	let options = result
		.semantic_changes
		.iter()
		.find(|change| change.item_path == ".#Options")
		.expect("changed Options interface");

	assert_eq!(
		options.assessment.as_ref().map(|value| value.outcome),
		Some(SemanticAnalysisOutcome::Breaking)
	);
	assert!(options.summary.contains("removed public member cache"));
}

#[test]
fn semantic_analysis_maps_source_entrypoints_to_emitted_declarations() {
	let result = analyze_typescript_fixture("source-entrypoint");
	let format = result
		.semantic_changes
		.iter()
		.find(|change| change.item_path == ".#format")
		.expect("added format export");

	assert_eq!(format.file_path, Path::new("dist/index.d.ts"));
	assert_eq!(
		format.assessment.as_ref().map(|value| value.outcome),
		Some(SemanticAnalysisOutcome::Additive)
	);
	assert!(result.warnings.is_empty());
}

#[test]
fn semantic_analysis_reports_paths_for_a_root_package() {
	let result = analyze_typescript_fixture_at("declaration-only", Path::new(""));
	let added = result
		.semantic_changes
		.iter()
		.find(|change| change.item_path == ".#format")
		.expect("added root-package declaration export");

	assert_eq!(added.file_path, Path::new("index.d.ts"));
}

#[test]
fn semantic_analysis_marks_wildcard_export_coverage_as_partial() {
	let result = analyze_typescript_fixture("wildcard");
	let partial = result
		.semantic_changes
		.iter()
		.find(|change| {
			change
				.assessment
				.as_ref()
				.is_some_and(|value| value.outcome == SemanticAnalysisOutcome::Inconclusive)
		})
		.expect("wildcard coverage finding");

	assert!(partial.summary.contains("inconclusive"));
	assert!(partial.assessment.as_ref().is_some_and(|assessment| {
		assessment.evidence.completeness == monochange_core::SemanticAnalysisCompleteness::Partial
			&& assessment
				.evidence
				.fallback_reason
				.as_deref()
				.is_some_and(|reason| reason.contains("wildcard export"))
	}));
	assert!(
		result
			.warnings
			.iter()
			.any(|warning| warning.contains("wildcard export"))
	);
}

#[test]
fn semantic_analysis_keeps_conditional_type_surfaces_separate() {
	let result = analyze_typescript_fixture("conditional");
	let outcome = |item_path: &str| {
		result
			.semantic_changes
			.iter()
			.find(|change| change.item_path == item_path)
			.and_then(|change| change.assessment.as_ref())
			.map(|assessment| assessment.outcome)
	};

	assert_eq!(
		outcome("./feature[import]#feature"),
		Some(SemanticAnalysisOutcome::Additive)
	);
	assert_eq!(
		outcome("./feature[require]#feature"),
		Some(SemanticAnalysisOutcome::Breaking)
	);
}

#[test]
fn semantic_analysis_reports_current_workspace_config_as_partial_input() {
	let result = analyze_typescript_fixture("inherited-config");
	let assessments = result
		.semantic_changes
		.iter()
		.filter_map(|change| change.assessment.as_ref())
		.collect::<Vec<_>>();

	assert!(
		assessments
			.iter()
			.any(|assessment| assessment.outcome == SemanticAnalysisOutcome::Compatible)
	);
	assert!(assessments.iter().any(|assessment| {
		assessment.outcome == SemanticAnalysisOutcome::Inconclusive
			&& assessment
				.evidence
				.fallback_reason
				.as_deref()
				.is_some_and(|reason| reason.contains("current workspace"))
	}));
}

#[test]
fn semantic_analysis_detects_transitive_private_type_changes() {
	let result = analyze_typescript_fixture("transitive");
	let parse = result
		.semantic_changes
		.iter()
		.find(|change| change.item_path == ".#parse")
		.expect("parse compatibility finding");

	assert_eq!(parse.before_signature, parse.after_signature);
	assert_eq!(
		parse.assessment.as_ref().map(|value| value.outcome),
		Some(SemanticAnalysisOutcome::Breaking)
	);
}

#[rstest]
#[case("advanced")]
#[case("conditional")]
#[case("invalid")]
#[case("wildcard")]
fn semantic_analysis_output_is_stable(#[case] scenario: &str) {
	let result = analyze_typescript_fixture(scenario);
	let changes = result
		.semantic_changes
		.iter()
		.map(|change| {
			serde_json::json!({
				"assessment": change.assessment,
				"category": change.category,
				"filePath": change.file_path,
				"itemKind": change.item_kind,
				"itemPath": change.item_path,
				"kind": change.kind,
				"summary": change.summary,
			})
		})
		.collect::<Vec<_>>();
	insta::assert_json_snapshot!(
		format!("typescript_analysis_{scenario}"),
		serde_json::json!({
			"analyzerId": result.analyzer_id,
			"changes": changes,
			"warnings": result.warnings,
		})
	);

	let signatures = result
		.semantic_changes
		.iter()
		.filter(|change| change.before_signature.is_some() || change.after_signature.is_some())
		.map(|change| {
			format!(
				"{}\nBEFORE\n{}\nAFTER\n{}",
				change.item_path,
				change
					.before_signature
					.as_deref()
					.unwrap_or("[not present]"),
				change.after_signature.as_deref().unwrap_or("[not present]")
			)
		})
		.collect::<Vec<_>>()
		.join("\n\n");
	insta::assert_snapshot!(
		format!("typescript_analysis_{scenario}_signatures"),
		if signatures.is_empty() {
			"[no changed signatures]"
		} else {
			&signatures
		}
	);
}

fn analyze_typescript_fixture(scenario: &str) -> PackageAnalysisResult {
	analyze_typescript_fixture_at(scenario, Path::new("virtual"))
}

fn analyze_typescript_fixture_at(scenario: &str, package_root: &Path) -> PackageAnalysisResult {
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/npm/typescript-analysis")
		.join(scenario);
	let before = fixture_snapshot(&fixture.join("before"), "before");
	let after = fixture_snapshot(&fixture.join("after"), "after");
	let changed_files = fixture_changes(&before, &after, package_root);
	let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let package = PackageRecord::new(
		Ecosystem::Npm,
		format!("@fixture/{scenario}"),
		repo_root.join(package_root).join("package.json"),
		repo_root.clone(),
		None,
		monochange_core::PublishState::Public,
	);
	let context = PackageAnalysisContext {
		repo_root: &repo_root,
		package: &package,
		detection_level: DetectionLevel::Semantic,
		changed_files: &changed_files,
		before_snapshot: Some(&before),
		after_snapshot: Some(&after),
	};

	semantic_analyzer()
		.analyze_package(&context)
		.unwrap_or_else(|error| panic!("analyze TypeScript fixture: {error}"))
}

fn fixture_snapshot(root: &Path, label: &str) -> PackageSnapshot {
	let mut files = WalkDir::new(root)
		.into_iter()
		.filter_map(Result::ok)
		.filter(|entry| entry.file_type().is_file())
		.map(|entry| {
			let path = entry
				.path()
				.strip_prefix(root)
				.unwrap_or_else(|error| panic!("fixture path: {error}"))
				.to_path_buf();
			let contents = fs::read_to_string(entry.path())
				.unwrap_or_else(|error| panic!("read {}: {error}", entry.path().display()));

			PackageSnapshotFile { path, contents }
		})
		.collect::<Vec<_>>();
	files.sort_by(|left, right| left.path.cmp(&right.path));

	PackageSnapshot {
		label: label.to_string(),
		files,
	}
}

fn fixture_changes(
	before: &PackageSnapshot,
	after: &PackageSnapshot,
	package_root: &Path,
) -> Vec<AnalyzedFileChange> {
	let paths = before
		.files
		.iter()
		.chain(&after.files)
		.map(|file| file.path.clone())
		.collect::<BTreeSet<_>>();

	paths
		.into_iter()
		.filter_map(|path| {
			let before_contents = before.file(&path).map(|file| file.contents.clone());
			let after_contents = after.file(&path).map(|file| file.contents.clone());

			if before_contents == after_contents {
				return None;
			}

			let kind = match (&before_contents, &after_contents) {
				(None, Some(_)) => FileChangeKind::Added,
				(Some(_), None) => FileChangeKind::Deleted,
				_ => FileChangeKind::Modified,
			};

			Some(AnalyzedFileChange {
				path: package_root.join(&path),
				package_path: path,
				kind,
				before_contents,
				after_contents,
			})
		})
		.collect()
}

#[test]
fn analyze_manifest_change_reports_export_dependency_and_metadata_diffs() {
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"@acme/web",
		PathBuf::from("/repo/packages/web/package.json"),
		PathBuf::from("/repo"),
		None,
		monochange_core::PublishState::Public,
	);
	let change = AnalyzedFileChange {
		path: PathBuf::from("packages/web/package.json"),
		package_path: PathBuf::from("package.json"),
		kind: FileChangeKind::Modified,
		before_contents: Some(
			serde_json::json!({
				"name": "@acme/web",
				"type": "module",
				"exports": "./src/index.ts",
				"dependencies": {"react": "18.2.0"}
			})
			.to_string(),
		),
		after_contents: Some(
			serde_json::json!({
				"name": "@acme/web",
				"type": "commonjs",
				"exports": {
					".": {"default": "./dist/index.js", "types": "./dist/index.d.ts"},
					"./cli": "./dist/cli.js"
				},
				"bin": {"acme-web": "./dist/cli.js"},
				"dependencies": {"react": "18.2.0", "zod": "3.24.0"},
				"scripts": {"build": "tsup"}
			})
			.to_string(),
		),
	};
	let mut warnings = Vec::new();
	let changes = analyze_manifest_change(&package, &change, &mut warnings);

	assert!(warnings.is_empty());
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Export
			&& change.item_path == "."
			&& change.kind == SemanticChangeKind::Modified
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Export
			&& change.item_path == "./cli"
			&& change.kind == SemanticChangeKind::Added
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Export
			&& change.item_path == "acme-web"
			&& change.item_kind == "command"
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Dependency
			&& change.item_path == "zod"
			&& change.kind == SemanticChangeKind::Added
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Metadata
			&& change.item_path == "type"
			&& change.kind == SemanticChangeKind::Modified
	}));
	assert!(changes.iter().any(|change| {
		change.category == SemanticChangeCategory::Metadata
			&& change.item_path == "script.build"
			&& change.kind == SemanticChangeKind::Added
	}));
}

#[test]
fn manifest_helpers_cover_parse_failures_removed_entries_and_scalar_bins() {
	let mut warnings = Vec::new();
	assert!(parse_manifest(Some("{"), Path::new("package.json"), &mut warnings).is_none());
	assert_eq!(warnings.len(), 1);

	let before = serde_json::json!({
		"exports": {".": "./dist/index.js", "./cli": "./dist/cli.js"},
		"bin": "./dist/index.js",
		"dependencies": {"react": "18"},
		"type": "module",
		"scripts": {"build": "tsup"}
	});
	let after = serde_json::json!({
		"exports": {".": "./dist/index.js"},
		"dependencies": {},
		"type": "commonjs"
	});

	let before_exports = extract_public_exports(&before, "pkg");
	let after_exports = extract_public_exports(&after, "pkg");
	let export_changes = compare_manifest_entries(
		SemanticChangeCategory::Export,
		Path::new("package.json"),
		&before_exports,
		&after_exports,
	);
	assert!(export_changes.iter().any(|change| {
		change.item_path == "./cli" && change.kind == SemanticChangeKind::Removed
	}));
	assert!(
		export_changes.iter().any(|change| {
			change.item_path == "pkg" && change.kind == SemanticChangeKind::Removed
		})
	);

	let metadata_changes = compare_manifest_entries(
		SemanticChangeCategory::Metadata,
		Path::new("package.json"),
		&extract_metadata_entries(&before),
		&extract_metadata_entries(&after),
	);
	assert!(metadata_changes.iter().any(|change| {
		change.item_path == "type" && change.kind == SemanticChangeKind::Modified
	}));
	assert!(metadata_changes.iter().any(|change| {
		change.item_path == "script.build" && change.kind == SemanticChangeKind::Removed
	}));

	let exports = extract_public_exports(
		&serde_json::json!({
			"exports": {".": "./dist/index.js", "./cli": "./dist/cli.js"},
			"bin": 7
		}),
		"pkg",
	);
	assert!(exports.contains_key("."));
	assert!(exports.contains_key("./cli"));
	assert!(!exports.contains_key("pkg"));

	assert_eq!(describe_json_value(&serde_json::json!(null)), "null");
	assert_eq!(describe_json_value(&serde_json::json!(true)), "true");
	assert_eq!(describe_json_value(&serde_json::json!(3)), "3");
	assert!(describe_json_value(&serde_json::json!(["a", "b"])).contains("a, b"));
	assert!(describe_json_value(&serde_json::json!({"b": 2, "a": 1})).contains("a=1"));
}

#[test]
fn api_snapshot_combines_ecmascript_and_manifest_items() {
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"@acme/web",
		PathBuf::from("/repo/packages/web/package.json"),
		PathBuf::from("/repo"),
		None,
		monochange_core::PublishState::Public,
	);
	let snapshot = PackageSnapshot {
		label: "HEAD".to_string(),
		files: vec![
			PackageSnapshotFile {
				path: PathBuf::from("package.json"),
				contents: serde_json::json!({
					"name": "@acme/web",
					"exports": {".": "./src/index.ts"},
					"bin": {"acme-web": "./bin.js"},
					"dependencies": {"react": "18"}
				})
				.to_string(),
			},
			PackageSnapshotFile {
				path: PathBuf::from("src/index.ts"),
				contents: "export function greet() {}".to_string(),
			},
		],
	};
	let context = PackageAnalysisContext {
		repo_root: Path::new("/repo"),
		package: &package,
		detection_level: DetectionLevel::Signature,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: Some(&snapshot),
	};

	let snapshot = api_snapshot(&context);

	assert_eq!(snapshot.package_id, "npm:packages/web/package.json");
	assert!(
		snapshot
			.items
			.iter()
			.any(|item| item.kind == "function" && item.path == "greet")
	);
	assert!(
		snapshot
			.items
			.iter()
			.any(|item| item.kind == "export" && item.path == ".")
	);
	assert!(
		snapshot
			.items
			.iter()
			.any(|item| item.kind == "command" && item.path == "acme-web")
	);
	assert!(
		snapshot
			.items
			.iter()
			.any(|item| item.kind == "dependency" && item.path == "react")
	);
}

#[test]
fn api_snapshot_handles_missing_manifest_file() {
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"@acme/web",
		PathBuf::from("/repo/packages/web/package.json"),
		PathBuf::from("/repo"),
		None,
		monochange_core::PublishState::Public,
	);
	let snapshot = PackageSnapshot {
		label: "HEAD".to_string(),
		files: Vec::new(),
	};
	let context = PackageAnalysisContext {
		repo_root: Path::new("/repo"),
		package: &package,
		detection_level: DetectionLevel::Signature,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: Some(&snapshot),
	};

	let snapshot = api_snapshot(&context);

	assert!(snapshot.items.is_empty());
}
