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
