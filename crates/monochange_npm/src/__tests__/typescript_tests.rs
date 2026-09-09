use std::io::Cursor;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use monochange_core::AnalyzedFileChange;
use monochange_core::BumpSeverity;
use monochange_core::DetectionLevel;
use monochange_core::Ecosystem;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageRecord;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::PublishState;
use monochange_core::SemanticAnalysisOutcome;

use super::*;

#[test]
fn analyzer_evidence_paths_must_stay_package_relative() {
	assert!(is_safe_relative_path(Path::new("dist/index.d.ts")));
	assert!(is_safe_relative_path(Path::new("./package.json")));
	assert!(!is_safe_relative_path(Path::new("../outside.ts")));
	assert!(!is_safe_relative_path(Path::new("/outside.ts")));
	assert!(!is_safe_relative_path(Path::new("C:\\outside.ts")));
	assert!(!is_safe_relative_path(Path::new("")));
}

#[test]
fn typescript_path_detection_covers_sources_and_declarations() {
	for path in ["index.ts", "index.tsx", "index.mts", "index.cts"] {
		assert!(is_typescript_target(path), "expected {path}");
	}
	for path in ["index.js", "index.jsx", "tsconfig.json", "types.txt"] {
		assert!(!is_typescript_target(path), "did not expect {path}");
	}
}

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
fn package_snapshot_paths_must_stay_relative() {
	let safe = PackageSnapshot {
		label: "safe".to_string(),
		files: vec![PackageSnapshotFile {
			path: Path::new("src/index.ts").to_path_buf(),
			contents: String::new(),
		}],
	};
	let unsafe_snapshot = PackageSnapshot {
		label: "unsafe".to_string(),
		files: vec![PackageSnapshotFile {
			path: Path::new("../outside.ts").to_path_buf(),
			contents: String::new(),
		}],
	};

	assert!(snapshot_paths_are_safe(&safe));
	assert!(!snapshot_paths_are_safe(&unsafe_snapshot));
}

#[test]
fn analyzer_process_output_is_bounded() {
	let within_limit = read_bounded(Cursor::new(b"1234"), 4)
		.unwrap_or_else(|error| panic!("bounded output: {error}"));
	assert_eq!(within_limit, b"1234");

	let error = read_bounded(Cursor::new(b"12345"), 4).expect_err("oversized output");
	assert!(error.to_string().contains("4 byte limit"));
}

#[test]
fn analyzer_process_reports_an_unavailable_node_runtime() {
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
	let missing_node = repo.path().join("missing-node");

	let Err(error) = run_typescript_with_node(&request, missing_node.as_os_str()) else {
		panic!("missing Node executable should fail");
	};

	assert!(error.contains("failed to start Node for TypeScript analysis"));
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
fn analyzer_process_reports_exit_and_protocol_failures() {
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

	let exit_error = run_typescript_process(
		&request,
		node,
		"process.stdin.resume(); process.stdin.on('end', () => { process.stderr.write('boom'); process.exit(3); });",
		Duration::from_secs(2),
		MAX_PROCESS_OUTPUT_BYTES,
		MAX_REQUEST_BYTES,
	)
	.expect_err("nonzero exit");
	assert!(exit_error.contains("exited with"));
	assert!(exit_error.contains("boom"));

	let protocol_error = run_typescript_process(
		&request,
		node,
		"process.stdin.resume(); process.stdin.on('end', () => { process.stdout.write('not json'); process.stderr.write('details'); });",
		Duration::from_secs(2),
		MAX_PROCESS_OUTPUT_BYTES,
		MAX_REQUEST_BYTES,
	)
	.expect_err("invalid protocol output");
	assert!(protocol_error.contains("failed to parse"));
	assert!(protocol_error.contains("details"));
}

#[test]
fn analyzer_process_cleans_up_when_the_child_closes_stdin() {
	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary repository: {error}"));
	let large_contents = "x".repeat(1024 * 1024);
	let before = PackageSnapshot {
		label: "before".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("large.ts"),
			contents: large_contents.clone(),
		}],
	};
	let after = PackageSnapshot {
		label: "after".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("large.ts"),
			contents: large_contents,
		}],
	};
	let request = TypeScriptRequest {
		repo_root: repo.path(),
		package_root: Path::new(""),
		package_name: "fixture",
		before: &before,
		after: &after,
	};

	let error = run_typescript_process(
		&request,
		OsStr::new("node"),
		"process.stdin.destroy(); process.exit(0);",
		Duration::from_secs(2),
		MAX_PROCESS_OUTPUT_BYTES,
		MAX_REQUEST_BYTES,
	)
	.expect_err("closed analyzer stdin");

	assert!(error.contains("failed to send TypeScript analyzer input"));
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

#[test]
fn analyzer_rejects_packages_outside_the_repository() {
	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary repository: {error}"));
	let outside = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary package: {error}"));
	let package = PackageRecord::new(
		Ecosystem::Npm,
		"fixture",
		outside.path().join("package.json"),
		outside.path().to_path_buf(),
		None,
		PublishState::Public,
	);
	let snapshot = PackageSnapshot {
		label: "fixture".to_string(),
		files: Vec::new(),
	};
	let context = PackageAnalysisContext {
		repo_root: repo.path(),
		package: &package,
		detection_level: DetectionLevel::Semantic,
		changed_files: &[],
		before_snapshot: Some(&snapshot),
		after_snapshot: Some(&snapshot),
	};

	let result = analyze_typescript_with(&context, Vec::new(), |_| {
		panic!("runner must not be called")
	});

	assert!(result.warnings[0].contains("outside the analyzed repository"));
}

#[test]
fn unsafe_helper_evidence_falls_back_without_trusting_the_path() {
	let response = TypeScriptResponse {
		version: Some("6.0.3".to_string()),
		fallback: false,
		coverage: "all exports".to_string(),
		fallback_reason: None,
		warnings: Vec::new(),
		changes: vec![TypeScriptChange {
			outcome: SemanticAnalysisOutcome::Breaking,
			suggested_bump: BumpSeverity::Major,
			kind: SemanticChangeKind::Modified,
			item_kind: "value".to_string(),
			item_path: ".#parse".to_string(),
			summary: "changed parse".to_string(),
			file_path: Path::new("../outside.ts").to_path_buf(),
			before_signature: None,
			after_signature: None,
			confidence: ApiConfidence::High,
			completeness: SemanticAnalysisCompleteness::Complete,
			coverage: "all exports".to_string(),
			fallback_reason: None,
		}],
	};

	let analysis = response_analysis(response, Vec::new());

	assert_eq!(analysis.changes.len(), 1);
	assert!(analysis.warnings[0].contains("unsafe evidence path"));
	assert_eq!(
		analysis.changes[0]
			.assessment
			.as_ref()
			.map(|value| value.outcome),
		Some(SemanticAnalysisOutcome::Inconclusive)
	);
}

#[test]
fn response_analysis_deduplicates_top_level_fallback_warnings() {
	let response = TypeScriptResponse {
		version: Some("6.0.3".to_string()),
		fallback: false,
		coverage: "partial".to_string(),
		fallback_reason: Some("workspace dependency used".to_string()),
		warnings: vec!["workspace dependency used".to_string()],
		changes: Vec::new(),
	};

	let analysis = response_analysis(response, Vec::new());

	assert_eq!(analysis.warnings, ["workspace dependency used"]);
}
