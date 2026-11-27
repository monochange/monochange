use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use monochange_core::AnalyzedFileChange;
use monochange_core::DetectionLevel;
use monochange_core::Ecosystem;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageRecord;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::PublishState;

use super::*;

#[test]
fn typed_surface_detection_uses_only_published_entrypoints() {
	let snapshot = |files: &[(&str, &str)]| {
		PackageSnapshot {
			label: "fixture".to_string(),
			files: files
				.iter()
				.map(|(path, contents)| {
					PackageSnapshotFile {
						path: PathBuf::from(path),
						contents: (*contents).to_string(),
					}
				})
				.collect(),
		}
	};

	assert!(snapshot_has_typed_surface(&snapshot(&[(
		"index.d.ts",
		"export {};",
	)])));
	assert!(!snapshot_has_typed_surface(&snapshot(&[(
		"package.json",
		"{",
	)])));
	assert!(snapshot_has_typed_surface(&snapshot(&[(
		"package.json",
		r#"{"typings":"./types.d.ts"}"#,
	)])));
	assert!(snapshot_has_typed_surface(&snapshot(&[(
		"package.json",
		r#"{"exports":[null,{"browser":"./src/index.ts"}]}"#,
	)])));
	assert!(snapshot_has_typed_surface(&snapshot(&[
		("package.json", r#"{"module":"./dist/index.js"}"#),
		("dist/index.d.ts", "export {};"),
	])));
	assert!(!snapshot_has_typed_surface(&snapshot(&[
		("package.json", r#"{"main":"README"}"#),
		("test/internal.d.ts", "export {};"),
	])));
	assert!(!snapshot_has_typed_surface(&snapshot(&[(
		"package.json",
		r#"{"exports":{"browser":false}}"#,
	)])));
	assert!(!target_has_declaration("./dist/styles.css", &snapshot(&[])));
}

#[test]
fn analyzer_process_enforces_request_output_and_time_limits() {
	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary repository: {error}"));
	let before = PackageSnapshot {
		label: "before".to_string(),
		files: Vec::new(),
	};
	let after = PackageSnapshot {
		label: "after".to_string(),
		files: Vec::new(),
	};
	let request = TypeScriptRequest {
		repo_root: repo.path(),
		package_root: Path::new(""),
		package_name: "fixture",
		before: &before,
		after: &after,
	};
	let node = OsStr::new("node");

	let request_error = run_typescript_process(&request, node, "", Duration::from_secs(1), 4, 0)
		.expect_err("request limit");
	assert!(request_error.contains("input exceeded"));

	let output_error = run_typescript_process(
		&request,
		node,
		"process.stdin.resume(); process.stdin.on('end', () => process.stdout.write('12345'));",
		Duration::from_secs(2),
		4,
		MAX_REQUEST_BYTES,
	)
	.expect_err("output limit");
	assert!(output_error.contains("output exceeded"));

	let timeout_error = run_typescript_process(
		&request,
		node,
		"process.stdin.resume(); process.stdin.on('end', () => setInterval(() => {}, 1000));",
		Duration::from_millis(50),
		MAX_PROCESS_OUTPUT_BYTES,
		MAX_REQUEST_BYTES,
	)
	.expect_err("process timeout");
	assert!(timeout_error.contains("exceeded its 0.05 second limit"));
}

#[test]
fn analyzer_boundary_fallbacks_preserve_uncertainty() {
	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary repository: {error}"));
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"fixture",
		repo.path().join("package/package.json"),
		repo.path().to_path_buf(),
		None,
		PublishState::Public,
	);
	let safe = PackageSnapshot {
		label: "safe".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("package.json"),
			contents: r#"{"types":"./index.d.ts"}"#.to_string(),
		}],
	};
	let changed_files: Vec<AnalyzedFileChange> = Vec::new();
	let context = |before, after| {
		PackageAnalysisContext {
			repo_root: repo.path(),
			package: &package,
			detection_level: DetectionLevel::Semantic,
			changed_files: &changed_files,
			before_snapshot: before,
			after_snapshot: after,
		}
	};

	let missing_before = analyze_typescript_with(&context(None, Some(&safe)), Vec::new(), |_| {
		panic!("runner must not be called")
	});
	assert!(missing_before.warnings[0].contains("before package snapshot"));
	let missing_after = analyze_typescript_with(&context(Some(&safe), None), Vec::new(), |_| {
		panic!("runner must not be called")
	});
	assert!(missing_after.warnings[0].contains("after package snapshot"));

	let unsafe_snapshot = PackageSnapshot {
		label: "unsafe".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("../outside.ts"),
			contents: String::new(),
		}],
	};
	let unsafe_result = analyze_typescript_with(
		&context(Some(&unsafe_snapshot), Some(&safe)),
		Vec::new(),
		|_| panic!("runner must not be called"),
	);
	assert!(unsafe_result.warnings[0].contains("unsafe path"));

	let runner_error =
		analyze_typescript_with(&context(Some(&safe), Some(&safe)), Vec::new(), |_| {
			Err("node failed".to_string())
		});
	assert_eq!(runner_error.warnings, ["node failed"]);
	let syntax_change = SemanticChange::new(
		SemanticChangeCategory::PublicApi,
		SemanticChangeKind::Modified,
		"function",
		"parse",
		"changed parse",
		"src/index.ts",
	);
	let syntax_fallback = analyze_typescript_with(
		&context(Some(&safe), Some(&safe)),
		vec![syntax_change],
		|_| Err("node failed".to_string()),
	);
	assert!(syntax_fallback.changes[0].assessment.is_some());

	let fallback_response =
		analyze_typescript_with(&context(Some(&safe), Some(&safe)), Vec::new(), |_| {
			Ok(TypeScriptResponse {
				version: Some("6.0.3".to_string()),
				fallback: true,
				coverage: "compiler unavailable".to_string(),
				fallback_reason: None,
				warnings: Vec::new(),
				changes: Vec::new(),
			})
		});
	assert_eq!(fallback_response.warnings, ["compiler unavailable"]);
}
