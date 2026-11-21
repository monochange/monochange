use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::FileChangeKind;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::SemanticAnalysisOutcome;
use walkdir::WalkDir;

use super::*;

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
