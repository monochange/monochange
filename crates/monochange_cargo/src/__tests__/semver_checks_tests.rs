use std::ffi::OsStr;

use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PackageSnapshot;
use monochange_core::PublishState;

use super::*;

fn cell(name: &str) -> CargoSemverMatrixCell {
	CargoSemverMatrixCell {
		name: name.to_string(),
		..CargoSemverMatrixCell::default()
	}
}

fn analyzer_with_cached_version() -> CargoSemverChecksAnalyzer {
	CargoSemverChecksAnalyzer {
		settings: CargoSemverChecksSettings {
			enabled: true,
			..CargoSemverChecksSettings::default()
		},
		engine: PathBuf::from("cargo-semver-checks"),
		state: Mutex::new(AnalyzerState {
			version: Some(Ok("0.47.0".to_string())),
			..AnalyzerState::default()
		}),
	}
}

fn package(root: &Path) -> PackageRecord {
	PackageRecord::new(
		Ecosystem::Cargo,
		"example",
		root.join("Cargo.toml"),
		root.to_path_buf(),
		None,
		PublishState::Public,
	)
}

fn fallback_reason(analysis: &CargoSemverAnalysis) -> Option<&str> {
	analysis
		.change
		.assessment
		.as_ref()
		.and_then(|assessment| assessment.evidence.fallback_reason.as_deref())
}

#[test]
fn parses_breaking_summary_and_lint_diagnostics() {
	let output = ProcessOutput {
		success: false,
		stdout: "--- failure struct_missing: publicly-visible struct removed ---\n".to_string(),
		stderr: "Summary semver requires new major version: 1 major and 0 minor checks failed"
			.to_string(),
	};
	let check = parse_matrix_output(&cell("default"), BTreeMap::new(), &output, Some("0.47.0"));

	assert_eq!(check.status, SemanticAnalyzerCheckStatus::Checked);
	assert_eq!(check.outcome, Some(SemanticAnalysisOutcome::Breaking));
	assert_eq!(check.suggested_bump, Some(BumpSeverity::Major));
	assert_eq!(check.diagnostics.len(), 1);
	let diagnostic = check
		.diagnostics
		.first()
		.unwrap_or_else(|| panic!("expected lint diagnostic"));
	assert_eq!(diagnostic.code, "struct_missing");
	assert_eq!(
		diagnostic.reference.as_deref(),
		Some(
			"https://github.com/obi1kenobi/cargo-semver-checks/tree/v0.47.0/src/lints/struct_missing.ron"
		)
	);
}

#[test]
fn parses_clean_summary_even_when_no_diagnostics_exist() {
	let output = ProcessOutput {
		success: true,
		stdout: String::new(),
		stderr: "Summary no semver update required".to_string(),
	};
	let check = parse_matrix_output(&cell("default"), BTreeMap::new(), &output, None);

	assert_eq!(check.status, SemanticAnalyzerCheckStatus::Checked);
	assert_eq!(check.outcome, Some(SemanticAnalysisOutcome::Compatible));
	assert_eq!(check.suggested_bump, Some(BumpSeverity::None));
	assert!(check.diagnostics.is_empty());
}

#[test]
fn parses_additive_summary_as_a_minor_bump() {
	let output = ProcessOutput {
		success: false,
		stdout: String::new(),
		stderr: "Summary semver requires new minor version: 0 major and 1 minor checks failed"
			.to_string(),
	};
	let check = parse_matrix_output(&cell("default"), BTreeMap::new(), &output, None);

	assert_eq!(check.outcome, Some(SemanticAnalysisOutcome::Additive));
	assert_eq!(check.suggested_bump, Some(BumpSeverity::Minor));
}

#[test]
fn rejects_unrecognized_process_output_instead_of_guessing() {
	let output = ProcessOutput {
		success: false,
		stdout: String::new(),
		stderr: "compiler failed".to_string(),
	};
	let check = parse_matrix_output(&cell("default"), BTreeMap::new(), &output, None);

	assert_eq!(check.status, SemanticAnalyzerCheckStatus::Failed);
	assert!(check.outcome.is_none());
	assert_eq!(
		check
			.diagnostics
			.first()
			.map(|diagnostic| diagnostic.code.as_str()),
		Some("analyzer_failed")
	);
}

#[test]
fn incomplete_matrix_keeps_a_proven_breaking_result() {
	let breaking = SemanticAnalyzerCheck::new(
		"default",
		SemanticAnalyzerCheckStatus::Checked,
		BTreeMap::new(),
	)
	.with_result(SemanticAnalysisOutcome::Breaking, BumpSeverity::Major);
	let failed = failed_check(
		&cell("all-features"),
		BTreeMap::new(),
		"target is not installed".to_string(),
	);
	let analysis = aggregate_analysis(vec![breaking, failed], Some("0.47.0".to_string()));
	let assessment = analysis
		.change
		.assessment
		.unwrap_or_else(|| panic!("matrix result should have an assessment"));

	assert_eq!(assessment.outcome, SemanticAnalysisOutcome::Breaking);
	assert_eq!(assessment.suggested_bump, BumpSeverity::Major);
	assert_eq!(assessment.confidence, ApiConfidence::High);
	assert_eq!(
		assessment.evidence.completeness,
		SemanticAnalysisCompleteness::Partial
	);
	assert!(!analysis.replace_syntax_changes);
}

#[test]
fn a_complete_compatible_matrix_can_replace_syntax_removals() {
	let checked = ["default", "all-features"]
		.into_iter()
		.map(|name| {
			SemanticAnalyzerCheck::new(name, SemanticAnalyzerCheckStatus::Checked, BTreeMap::new())
				.with_result(SemanticAnalysisOutcome::Compatible, BumpSeverity::None)
		})
		.collect();
	let analysis = aggregate_analysis(checked, Some("0.47.0".to_string()));

	assert!(analysis.replace_syntax_changes);
	assert_eq!(
		analysis
			.change
			.assessment
			.as_ref()
			.map(|assessment| assessment.suggested_bump),
		Some(BumpSeverity::None)
	);
	assert!(analysis.change.summary.contains("no required version bump"));
}

#[test]
fn a_complete_additive_matrix_can_replace_conservative_syntax_modifications() {
	let additive = SemanticAnalyzerCheck::new(
		"default",
		SemanticAnalyzerCheckStatus::Checked,
		BTreeMap::new(),
	)
	.with_result(SemanticAnalysisOutcome::Additive, BumpSeverity::Minor);
	let analysis = aggregate_analysis(vec![additive], Some("0.47.0".to_string()));

	assert!(analysis.replace_syntax_changes);
	assert_eq!(
		analysis
			.change
			.assessment
			.as_ref()
			.map(|assessment| assessment.suggested_bump),
		Some(BumpSeverity::Minor)
	);
	assert!(analysis.change.summary.contains("additive Rust API change"));
}

#[test]
fn matrix_arguments_preserve_endpoint_specific_features() {
	let cell = CargoSemverMatrixCell {
		name: "wasm".to_string(),
		feature_mode: CargoSemverFeatureMode::None,
		features: vec!["serde".to_string()],
		baseline_features: vec!["old-api".to_string()],
		current_features: vec!["new-api".to_string()],
		target: Some("wasm32-unknown-unknown".to_string()),
	};
	let mut command = Command::new("cargo-semver-checks");
	append_matrix_args(&mut command, &cell);
	let args = command.get_args().collect::<Vec<_>>();

	assert!(args.contains(&OsStr::new("--only-explicit-features")));
	assert!(args.windows(2).any(|pair| pair == ["--features", "serde"]));
	assert!(
		args.windows(2)
			.any(|pair| pair == ["--baseline-features", "old-api"])
	);
	assert!(
		args.windows(2)
			.any(|pair| pair == ["--current-features", "new-api"])
	);
	assert!(
		args.windows(2)
			.any(|pair| pair == ["--target", "wasm32-unknown-unknown"])
	);
}

#[test]
fn heuristic_feature_mode_adds_no_selection_flag_and_is_reported() {
	let heuristic = CargoSemverMatrixCell {
		name: "heuristic".to_string(),
		feature_mode: CargoSemverFeatureMode::Heuristic,
		..CargoSemverMatrixCell::default()
	};
	let mut command = Command::new("cargo-semver-checks");
	append_matrix_args(&mut command, &heuristic);

	assert!(command.get_args().next().is_none());
	assert_eq!(feature_mode_name(heuristic.feature_mode), "heuristic");
}

#[test]
fn default_and_all_feature_modes_use_their_exact_flags() {
	for (feature_mode, name, flag) in [
		(
			CargoSemverFeatureMode::Default,
			"default",
			"--default-features",
		),
		(CargoSemverFeatureMode::All, "all", "--all-features"),
	] {
		let cell = CargoSemverMatrixCell {
			feature_mode,
			..CargoSemverMatrixCell::default()
		};
		let mut command = Command::new("cargo-semver-checks");
		append_matrix_args(&mut command, &cell);

		assert_eq!(command.get_args().collect::<Vec<_>>(), [OsStr::new(flag)]);
		assert_eq!(feature_mode_name(feature_mode), name);
	}

	assert_eq!(feature_mode_name(CargoSemverFeatureMode::None), "none");
}

#[test]
fn repository_path_validation_rejects_escape_and_absolute_paths() {
	assert!(safe_repository_path(Path::new("crates/api/Cargo.toml")));
	assert!(!safe_repository_path(Path::new("../outside/Cargo.toml")));
	assert!(!safe_repository_path(Path::new("/outside/Cargo.toml")));
}

#[test]
fn git_object_validation_accepts_only_complete_sha1_or_sha256_ids() {
	assert!(is_git_object_id(&"a".repeat(40)));
	assert!(is_git_object_id(&"A".repeat(64)));
	assert!(!is_git_object_id(&"a".repeat(39)));
	assert!(!is_git_object_id(&"a".repeat(41)));
	assert!(!is_git_object_id(&format!("{}g", "a".repeat(39))));
}

#[test]
fn materialized_git_tree_is_isolated_from_working_tree_changes() {
	let repository = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(repository.path().join("Cargo.toml"), "before")
		.unwrap_or_else(|error| panic!("write manifest: {error}"));
	run_git(repository.path(), &["init"])
		.unwrap_or_else(|error| panic!("initialize repository: {error}"));
	run_git(repository.path(), &["add", "Cargo.toml"])
		.unwrap_or_else(|error| panic!("stage manifest: {error}"));
	let tree = git_stdout(repository.path(), &["write-tree"])
		.unwrap_or_else(|error| panic!("write tree: {error}"));
	std::fs::write(repository.path().join("Cargo.toml"), "after")
		.unwrap_or_else(|error| panic!("change manifest: {error}"));
	let mut state = AnalyzerState::default();
	let materialized = materialize_workspace(&mut state, repository.path(), &tree)
		.unwrap_or_else(|error| panic!("materialize tree: {error}"));

	assert_eq!(
		std::fs::read_to_string(materialized.join("Cargo.toml"))
			.unwrap_or_else(|error| panic!("read materialized manifest: {error}")),
		"before"
	);
	assert_eq!(
		std::fs::read_to_string(repository.path().join("Cargo.toml"))
			.unwrap_or_else(|error| panic!("read working manifest: {error}")),
		"after"
	);
}

#[test]
fn analyzer_reports_missing_endpoints_and_paths_without_running_the_tool() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let package = package(root.path());
	let snapshot = PackageSnapshot {
		label: "0".repeat(40),
		files: Vec::new(),
	};
	let analyzer = analyzer_with_cached_version();
	let missing_before = PackageAnalysisContext {
		repo_root: root.path(),
		package: &package,
		detection_level: DetectionLevel::Semantic,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: Some(&snapshot),
	};
	let missing_after = PackageAnalysisContext {
		before_snapshot: Some(&snapshot),
		after_snapshot: None,
		..missing_before
	};

	assert_eq!(
		fallback_reason(&analyzer.analyze_enabled(&missing_before)),
		Some("the baseline snapshot is unavailable")
	);
	assert_eq!(
		fallback_reason(&analyzer.analyze_enabled(&missing_after)),
		Some("the candidate snapshot is unavailable")
	);

	let outside = PackageRecord::new(
		Ecosystem::Cargo,
		"outside",
		PathBuf::from("/outside/Cargo.toml"),
		PathBuf::from("/outside"),
		None,
		PublishState::Public,
	);
	let outside_context = PackageAnalysisContext {
		package: &outside,
		before_snapshot: Some(&snapshot),
		after_snapshot: Some(&snapshot),
		..missing_before
	};
	assert_eq!(
		fallback_reason(&analyzer.analyze_enabled(&outside_context)),
		Some("the package manifest is outside the repository")
	);
}

#[test]
fn analyzer_reports_invalid_trees_and_missing_endpoint_manifests() {
	let repository = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(repository.path().join("README.md"), "fixture")
		.unwrap_or_else(|error| panic!("write fixture: {error}"));
	run_git(repository.path(), &["init"])
		.unwrap_or_else(|error| panic!("initialize repository: {error}"));
	run_git(repository.path(), &["add", "README.md"])
		.unwrap_or_else(|error| panic!("stage fixture: {error}"));
	let tree = git_stdout(repository.path(), &["write-tree"])
		.unwrap_or_else(|error| panic!("write tree: {error}"));
	std::fs::write(
		repository.path().join("Cargo.toml"),
		"[package]\nname = \"example\"",
	)
	.unwrap_or_else(|error| panic!("write unstaged manifest: {error}"));
	let package = package(repository.path());
	let valid = PackageSnapshot {
		label: tree,
		files: Vec::new(),
	};
	let invalid = PackageSnapshot {
		label: "HEAD".to_string(),
		files: Vec::new(),
	};
	let context = |before, after| {
		PackageAnalysisContext {
			repo_root: repository.path(),
			package: &package,
			detection_level: DetectionLevel::Semantic,
			changed_files: &[],
			before_snapshot: before,
			after_snapshot: after,
		}
	};

	let invalid_before =
		analyzer_with_cached_version().analyze_enabled(&context(Some(&invalid), Some(&valid)));
	let invalid_before_reason = fallback_reason(&invalid_before);
	assert!(
		invalid_before_reason.is_some_and(|reason| reason.contains("not a trusted Git tree")),
		"unexpected fallback: {invalid_before_reason:?}"
	);
	let invalid_after =
		analyzer_with_cached_version().analyze_enabled(&context(Some(&valid), Some(&invalid)));
	assert!(
		fallback_reason(&invalid_after)
			.is_some_and(|reason| reason.contains("not a trusted Git tree"))
	);
	let missing_manifests =
		analyzer_with_cached_version().analyze_enabled(&context(Some(&valid), Some(&valid)));
	assert_eq!(
		fallback_reason(&missing_manifests),
		Some("the package manifest does not exist at both comparison endpoints")
	);
}

#[test]
fn git_and_matrix_process_failures_are_explicit() {
	let missing = Path::new("/monochange/definitely-missing-repository");
	let clone_error =
		materialize_workspace(&mut AnalyzerState::default(), missing, &"0".repeat(40))
			.expect_err("a missing repository should fail to clone");
	assert!(clone_error.contains("git clone"));

	let not_a_repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let git_error = git_stdout(not_a_repo.path(), &["write-tree"])
		.expect_err("write-tree should fail outside a repository");
	assert!(git_error.contains("git write-tree failed"));

	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest = root.path().join("Cargo.toml");
	let run_context = MatrixRunContext {
		engine: Path::new("/monochange/definitely-missing-cargo-semver-checks"),
		timeout_seconds: 1,
		current_root: root.path(),
		current_manifest: &manifest,
		baseline_root: root.path(),
		package_name: "example",
		version: Some("0.47.0"),
	};
	let check = run_matrix_cell(&cell("missing"), &run_context);
	assert_eq!(check.status, SemanticAnalyzerCheckStatus::Failed);
}

#[test]
fn analyzer_version_caches_failed_process_results() {
	let mut state = AnalyzerState::default();
	let first =
		analyzer_version(&mut state, Path::new("git")).expect_err("git is not cargo-semver-checks");
	let second = analyzer_version(&mut state, Path::new("git"))
		.expect_err("the cached failure should be returned");

	assert!(first.contains("could not start"));
	assert_eq!(first, second);
}

#[test]
fn poisoned_analyzer_state_returns_unsupported_evidence() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let package = package(root.path());
	let snapshot = PackageSnapshot {
		label: "0".repeat(40),
		files: Vec::new(),
	};
	let analyzer = analyzer_with_cached_version();
	std::thread::scope(|scope| {
		let handle = scope.spawn(|| {
			let _guard = analyzer
				.state
				.lock()
				.unwrap_or_else(|error| panic!("lock state: {error}"));
			panic!("poison analyzer state");
		});
		assert!(handle.join().is_err());
	});
	let context = PackageAnalysisContext {
		repo_root: root.path(),
		package: &package,
		detection_level: DetectionLevel::Semantic,
		changed_files: &[],
		before_snapshot: Some(&snapshot),
		after_snapshot: Some(&snapshot),
	};
	let analysis = analyzer
		.analyze(&context)
		.unwrap_or_else(|| panic!("enabled analyzer should return evidence"));

	assert_eq!(
		fallback_reason(&analysis),
		Some("cargo-semver-checks analyzer state was poisoned")
	);
}

#[test]
fn parser_ignores_malformed_diagnostics_and_output_limits_fail_closed() {
	let diagnostics = parse_diagnostics(
		"--- failure : missing code ---\n--- failure missing_colon ---\nnot a diagnostic",
		None,
	);
	assert!(diagnostics.is_empty());

	let reader = std::thread::spawn(|| {
		Ok::<Vec<u8>, std::io::Error>(vec![0; MAX_PROCESS_OUTPUT_BYTES as usize + 1])
	});
	let error = join_reader(reader, "stdout").expect_err("oversized output should fail");
	assert!(error.contains("exceeded the 8 MiB output limit"));
}

#[test]
fn empty_and_partial_matrices_preserve_uncertainty() {
	let empty = aggregate_analysis(Vec::new(), None);
	let empty_assessment = empty
		.change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("empty matrix should have an assessment"));
	assert_eq!(
		empty_assessment.evidence.completeness,
		SemanticAnalysisCompleteness::Unsupported
	);
	assert_eq!(
		empty_assessment.outcome,
		SemanticAnalysisOutcome::Inconclusive
	);
	assert_eq!(empty_assessment.confidence, ApiConfidence::Low);

	let compatible = SemanticAnalyzerCheck::new(
		"default",
		SemanticAnalyzerCheckStatus::Checked,
		BTreeMap::new(),
	)
	.with_result(SemanticAnalysisOutcome::Compatible, BumpSeverity::None);
	let failed = failed_check(
		&cell("target"),
		BTreeMap::new(),
		"target missing".to_string(),
	);
	let partial = aggregate_analysis(vec![compatible, failed], None);
	let partial_assessment = partial
		.change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("partial matrix should have an assessment"));
	assert_eq!(partial_assessment.confidence, ApiConfidence::Low);
	assert_eq!(
		partial_assessment.evidence.completeness,
		SemanticAnalysisCompleteness::Partial
	);

	let unavailable = failed_analysis_with_version("unavailable", Some("0.47.0".to_string()));
	assert_eq!(
		unavailable
			.change
			.assessment
			.as_ref()
			.and_then(|assessment| assessment.evidence.version.as_deref()),
		Some("0.47.0")
	);
}

#[test]
fn unavailable_tool_produces_explicit_unsupported_evidence() {
	let root = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"example",
		root.path().join("Cargo.toml"),
		root.path().to_path_buf(),
		None,
		PublishState::Public,
	);
	let before = PackageSnapshot {
		label: "0000000000000000000000000000000000000000".to_string(),
		files: Vec::new(),
	};
	let after = before.clone();
	let context = PackageAnalysisContext {
		repo_root: root.path(),
		package: &package,
		detection_level: DetectionLevel::Semantic,
		changed_files: &[],
		before_snapshot: Some(&before),
		after_snapshot: Some(&after),
	};
	let analyzer = CargoSemverChecksAnalyzer {
		settings: CargoSemverChecksSettings {
			enabled: true,
			..CargoSemverChecksSettings::default()
		},
		engine: PathBuf::from("/monochange/definitely-missing-cargo-semver-checks"),
		state: Mutex::new(AnalyzerState::default()),
	};
	let analysis = analyzer
		.analyze(&context)
		.unwrap_or_else(|| panic!("enabled analyzer should return evidence"));
	let evidence = &analysis
		.change
		.assessment
		.as_ref()
		.unwrap_or_else(|| panic!("fallback should have an assessment"))
		.evidence;

	assert_eq!(
		evidence.completeness,
		SemanticAnalysisCompleteness::Unsupported
	);
	assert!(evidence.fallback_reason.is_some());
	assert_eq!(
		evidence.checks.first().map(|check| check.status),
		Some(SemanticAnalyzerCheckStatus::Skipped)
	);
}

#[cfg(unix)]
#[test]
fn process_runner_stops_a_timed_out_analyzer() {
	let mut command = Command::new("sh");
	command.args(["-c", "sleep 2"]);
	let error = run_process(&mut command, Duration::from_millis(20))
		.expect_err("the child should time out");

	assert!(error.contains("exceeded the 0 second timeout"));
}
