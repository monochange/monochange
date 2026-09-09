use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use monochange_core::ApiConfidence;
use monochange_core::BumpSeverity;
use monochange_core::CargoSemverChecksSettings;
use monochange_core::CargoSemverFeatureMode;
use monochange_core::CargoSemverMatrixCell;
use monochange_core::DetectionLevel;
use monochange_core::PackageAnalysisContext;
use monochange_core::SemanticAnalysisCompleteness;
use monochange_core::SemanticAnalysisOutcome;
use monochange_core::SemanticAnalyzerCheck;
use monochange_core::SemanticAnalyzerCheckStatus;
use monochange_core::SemanticAnalyzerDiagnostic;
use monochange_core::SemanticAnalyzerEvidence;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeAssessment;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use tempfile::TempDir;
use wait_timeout::ChildExt as _;

use crate::CARGO_MANIFEST_FILE;

const ANALYZER_ID: &str = "cargo/cargo-semver-checks";
const ENGINE: &str = "cargo-semver-checks";
const MAX_PROCESS_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct CargoSemverChecksAnalyzer {
	settings: CargoSemverChecksSettings,
	engine: PathBuf,
	state: Mutex<AnalyzerState>,
}

#[derive(Debug, Default)]
struct AnalyzerState {
	version: Option<Result<String, String>>,
	workspaces: BTreeMap<String, MaterializedWorkspace>,
	checks: BTreeMap<MatrixCacheKey, SemanticAnalyzerCheck>,
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct MatrixCacheKey {
	before: String,
	after: String,
	manifest: PathBuf,
	package_name: String,
	cell: CargoSemverMatrixCell,
}

#[derive(Debug)]
struct MaterializedWorkspace {
	_root: TempDir,
	path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct CargoSemverAnalysis {
	pub(crate) change: SemanticChange,
	pub(crate) replace_syntax_changes: bool,
}

#[derive(Debug)]
struct ProcessOutput {
	success: bool,
	stdout: String,
	stderr: String,
}

impl CargoSemverChecksAnalyzer {
	pub(crate) fn new(settings: CargoSemverChecksSettings) -> Self {
		Self {
			settings,
			engine: PathBuf::from(ENGINE),
			state: Mutex::new(AnalyzerState::default()),
		}
	}

	pub(crate) fn analyze(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> Option<CargoSemverAnalysis> {
		if !self.settings.enabled || context.detection_level != DetectionLevel::Semantic {
			return None;
		}

		Some(self.analyze_enabled(context))
	}

	fn analyze_enabled(&self, context: &PackageAnalysisContext<'_>) -> CargoSemverAnalysis {
		let Ok(mut state) = self.state.lock() else {
			return failed_analysis("cargo-semver-checks analyzer state was poisoned");
		};
		let version = analyzer_version(&mut state, &self.engine);
		let version_text = version.as_ref().ok().cloned();
		let before = context
			.before_snapshot
			.map(|snapshot| snapshot.label.as_str());
		let after = context
			.after_snapshot
			.map(|snapshot| snapshot.label.as_str());
		let Some(before) = before else {
			return failed_analysis_with_version(
				"the baseline snapshot is unavailable",
				version_text,
			);
		};
		let Some(after) = after else {
			return failed_analysis_with_version(
				"the candidate snapshot is unavailable",
				version_text,
			);
		};
		if let Err(reason) = version {
			return failed_analysis_with_version(&reason, None);
		}

		let manifest_relative = match context.package.relative_manifest_path(context.repo_root) {
			Some(path) if safe_repository_path(&path) => path,
			_ => {
				return failed_analysis_with_version(
					"the package manifest is outside the repository",
					version_text,
				);
			}
		};
		let package_relative = manifest_relative.parent().unwrap_or_else(|| Path::new(""));
		let before_root = match materialize_workspace(&mut state, context.repo_root, before) {
			Ok(path) => path,
			Err(reason) => return failed_analysis_with_version(&reason, version_text),
		};
		let after_root = match materialize_workspace(&mut state, context.repo_root, after) {
			Ok(path) => path,
			Err(reason) => return failed_analysis_with_version(&reason, version_text),
		};
		let current_manifest = after_root.join(&manifest_relative);
		let baseline_root = before_root.join(package_relative);
		if !current_manifest.is_file() || !baseline_root.join(CARGO_MANIFEST_FILE).is_file() {
			return failed_analysis_with_version(
				"the package manifest does not exist at both comparison endpoints",
				version_text,
			);
		}

		let mut checks = Vec::with_capacity(self.settings.matrix.len());
		let run_context = MatrixRunContext {
			engine: &self.engine,
			timeout_seconds: self.settings.timeout_seconds,
			current_root: &after_root,
			current_manifest: &current_manifest,
			baseline_root: &baseline_root,
			package_name: &context.package.name,
			version: version_text.as_deref(),
		};
		for cell in &self.settings.matrix {
			let key = MatrixCacheKey {
				before: before.to_string(),
				after: after.to_string(),
				manifest: manifest_relative.clone(),
				package_name: context.package.name.clone(),
				cell: cell.clone(),
			};
			let check = state.checks.get(&key).cloned().unwrap_or_else(|| {
				let check = run_matrix_cell(cell, &run_context);
				state.checks.insert(key, check.clone());
				check
			});
			checks.push(check);
		}

		aggregate_analysis(checks, version_text)
	}
}

fn analyzer_version(state: &mut AnalyzerState, engine: &Path) -> Result<String, String> {
	if let Some(version) = &state.version {
		return version.clone();
	}
	let output = run_process(
		Command::new(engine).args(["semver-checks", "--version"]),
		Duration::from_secs(10),
	);
	let version = match output {
		Ok(output) if output.success => parse_version(&output.stdout, &output.stderr),
		Ok(output) => {
			Err(format!(
				"cargo-semver-checks could not start: {}",
				concise_process_detail(&output.stdout, &output.stderr)
			))
		}
		Err(reason) => Err(reason),
	};
	state.version = Some(version.clone());
	version
}

fn parse_version(stdout: &str, stderr: &str) -> Result<String, String> {
	stdout
		.lines()
		.chain(stderr.lines())
		.find_map(|line| line.trim().strip_prefix("cargo-semver-checks "))
		.map(str::trim)
		.filter(|version| !version.is_empty())
		.map(ToString::to_string)
		.ok_or_else(|| "cargo-semver-checks did not report its version".to_string())
}

fn materialize_workspace(
	state: &mut AnalyzerState,
	repo_root: &Path,
	label: &str,
) -> Result<PathBuf, String> {
	if let Some(workspace) = state.workspaces.get(label) {
		return Ok(workspace.path.clone());
	}
	let revision = if is_git_object_id(label) {
		label.to_string()
	} else {
		return Err(format!(
			"snapshot label `{label}` is not a trusted Git tree"
		));
	};
	let temporary = tempfile::Builder::new()
		// patch-coverage:ignore-start -- exercising temporary-directory allocation failure requires exhausting OS resources.
		.prefix("monochange-cargo-semver-")
		.tempdir()
		.map_err(|error| format!("failed to allocate an analyzer workspace: {error}"))?;
	// patch-coverage:ignore-end
	run_git(
		repo_root,
		&[
			"clone",
			"--no-checkout",
			"--shared",
			"--",
			&repo_root.to_string_lossy(),
			&temporary.path().to_string_lossy(),
		],
	)?;
	run_git(temporary.path(), &["read-tree", &revision])?;
	run_git(temporary.path(), &["checkout-index", "--all", "--force"])?;
	let path = temporary.path().to_path_buf();
	state.workspaces.insert(
		label.to_string(),
		MaterializedWorkspace {
			_root: temporary,
			path: path.clone(),
		},
	);
	Ok(path)
}

fn git_stdout(repo_root: &Path, args: &[&str]) -> Result<String, String> {
	let output = Command::new("git")
		.current_dir(repo_root)
		.args(args)
		.output()
		.map_err(|error| format!("failed to run git {}: {error}", args.join(" ")))?;
	if !output.status.success() {
		return Err(format!(
			"git {} failed: {}",
			args.join(" "),
			String::from_utf8_lossy(&output.stderr).trim()
		));
	}
	Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<(), String> {
	git_stdout(repo_root, args).map(drop)
}

fn safe_repository_path(path: &Path) -> bool {
	!path.is_absolute()
		&& path.components().all(|component| {
			matches!(
				component,
				std::path::Component::Normal(_) | std::path::Component::CurDir
			)
		})
}

fn is_git_object_id(value: &str) -> bool {
	matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

struct MatrixRunContext<'a> {
	engine: &'a Path,
	timeout_seconds: u64,
	current_root: &'a Path,
	current_manifest: &'a Path,
	baseline_root: &'a Path,
	package_name: &'a str,
	version: Option<&'a str>,
}

fn run_matrix_cell(
	cell: &CargoSemverMatrixCell,
	context: &MatrixRunContext<'_>,
) -> SemanticAnalyzerCheck {
	let configuration = matrix_configuration(cell);
	let target_dir = match tempfile::Builder::new()
		// patch-coverage:ignore-start -- exercising temporary-directory allocation failure requires exhausting OS resources.
		.prefix("monochange-cargo-semver-target-")
		.tempdir()
	{
		Ok(directory) => directory,
		Err(error) => {
			return failed_check(
				cell,
				configuration,
				format!("failed to allocate an isolated Cargo target directory: {error}"),
			);
		}
	};
	// patch-coverage:ignore-end
	let mut command = Command::new(context.engine);
	command
		.current_dir(context.current_root)
		.env("CARGO_TARGET_DIR", target_dir.path())
		.env("CARGO_TERM_COLOR", "never")
		.args(["semver-checks", "check-release", "--manifest-path"])
		.arg(context.current_manifest)
		.arg("--baseline-root")
		.arg(context.baseline_root)
		.args([
			"--package",
			context.package_name,
			"--release-type",
			"patch",
			"--color",
			"never",
		]);
	append_matrix_args(&mut command, cell);

	match run_process(&mut command, Duration::from_secs(context.timeout_seconds)) {
		Ok(output) => parse_matrix_output(cell, configuration, &output, context.version),
		Err(reason) => failed_check(cell, configuration, reason),
	}
}

fn append_matrix_args(command: &mut Command, cell: &CargoSemverMatrixCell) {
	match cell.feature_mode {
		CargoSemverFeatureMode::Default => {
			command.arg("--default-features");
		}
		CargoSemverFeatureMode::All => {
			command.arg("--all-features");
		}
		CargoSemverFeatureMode::None => {
			command.arg("--only-explicit-features");
		}
		_ => {}
	}
	append_feature_arg(command, "--features", &cell.features);
	append_feature_arg(command, "--baseline-features", &cell.baseline_features);
	append_feature_arg(command, "--current-features", &cell.current_features);
	if let Some(target) = &cell.target {
		command.args(["--target", target]);
	}
}

fn append_feature_arg(command: &mut Command, flag: &str, features: &[String]) {
	if !features.is_empty() {
		command.args([flag, &features.join(",")]);
	}
}

fn matrix_configuration(cell: &CargoSemverMatrixCell) -> BTreeMap<String, String> {
	let mut configuration = BTreeMap::from([
		(
			"featureMode".to_string(),
			feature_mode_name(cell.feature_mode).to_string(),
		),
		(
			"target".to_string(),
			cell.target.clone().unwrap_or_else(|| "host".to_string()),
		),
	]);
	for (key, features) in [
		("features", &cell.features),
		("baselineFeatures", &cell.baseline_features),
		("currentFeatures", &cell.current_features),
	] {
		if !features.is_empty() {
			configuration.insert(key.to_string(), features.join(","));
		}
	}
	configuration
}

fn feature_mode_name(mode: CargoSemverFeatureMode) -> &'static str {
	match mode {
		CargoSemverFeatureMode::Default => "default",
		CargoSemverFeatureMode::All => "all",
		CargoSemverFeatureMode::None => "none",
		CargoSemverFeatureMode::Heuristic => "heuristic",
		// patch-coverage:ignore-start -- the enum is non-exhaustive for downstream compatibility; every current variant is tested.
		_ => "unknown",
		// patch-coverage:ignore-end
	}
}

fn run_process(command: &mut Command, timeout: Duration) -> Result<ProcessOutput, String> {
	let mut child = command
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.map_err(|error| format!("failed to start cargo-semver-checks: {error}"))?;
	let stdout = child
		.stdout
		.take()
		.ok_or_else(|| "failed to capture cargo-semver-checks stdout".to_string())?;
	let stderr = child
		.stderr
		.take()
		.ok_or_else(|| "failed to capture cargo-semver-checks stderr".to_string())?;
	let stdout_reader = std::thread::spawn(move || read_bounded(stdout));
	let stderr_reader = std::thread::spawn(move || read_bounded(stderr));
	let Some(status) = child
		.wait_timeout(timeout)
		.map_err(|error| format!("failed while waiting for cargo-semver-checks: {error}"))?
	else {
		let _ = child.kill();
		let _ = child.wait();
		let _ = stdout_reader.join();
		let _ = stderr_reader.join();
		return Err(format!(
			"cargo-semver-checks exceeded the {} second timeout",
			timeout.as_secs()
		));
	};
	let stdout = join_reader(stdout_reader, "stdout")?;
	let stderr = join_reader(stderr_reader, "stderr")?;
	Ok(ProcessOutput {
		success: status.success(),
		stdout,
		stderr,
	})
}

fn read_bounded(reader: impl std::io::Read) -> Result<Vec<u8>, std::io::Error> {
	let mut output = Vec::new();
	reader
		.take(MAX_PROCESS_OUTPUT_BYTES + 1)
		.read_to_end(&mut output)?;
	Ok(output)
}

fn join_reader(
	reader: std::thread::JoinHandle<Result<Vec<u8>, std::io::Error>>,
	stream: &str,
) -> Result<String, String> {
	let bytes = reader
		.join()
		.map_err(|_| format!("cargo-semver-checks {stream} reader panicked"))?
		.map_err(|error| format!("failed to read cargo-semver-checks {stream}: {error}"))?;
	if bytes.len() > MAX_PROCESS_OUTPUT_BYTES as usize {
		return Err(format!(
			"cargo-semver-checks {stream} exceeded the 8 MiB output limit"
		));
	}
	Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn parse_matrix_output(
	cell: &CargoSemverMatrixCell,
	configuration: BTreeMap<String, String>,
	output: &ProcessOutput,
	version: Option<&str>,
) -> SemanticAnalyzerCheck {
	let combined = format!("{}\n{}", output.stdout, output.stderr);
	let (outcome, suggested_bump) = if combined.contains("semver requires new major version") {
		(SemanticAnalysisOutcome::Breaking, BumpSeverity::Major)
	} else if combined.contains("semver requires new minor version") {
		(SemanticAnalysisOutcome::Additive, BumpSeverity::Minor)
	} else if combined.contains("no semver update required") {
		(SemanticAnalysisOutcome::Compatible, BumpSeverity::None)
	} else {
		return failed_check(
			cell,
			configuration,
			format!(
				"cargo-semver-checks returned no recognized summary{}: {}",
				if output.success { "" } else { " after failing" },
				concise_process_detail(&output.stdout, &output.stderr)
			),
		);
	};
	let diagnostics = parse_diagnostics(&combined, version);
	SemanticAnalyzerCheck::new(
		cell.name.clone(),
		SemanticAnalyzerCheckStatus::Checked,
		configuration,
	)
	.with_result(outcome, suggested_bump)
	.with_diagnostics(diagnostics)
}

fn parse_diagnostics(output: &str, version: Option<&str>) -> Vec<SemanticAnalyzerDiagnostic> {
	let mut diagnostics = output
		.lines()
		.filter_map(|line| {
			let body = line
				.trim()
				.strip_prefix("--- failure ")?
				.strip_suffix(" ---")?;
			let (code, message) = body.split_once(':')?;
			let code = code.trim();
			if code.is_empty() {
				return None;
			}
			let diagnostic = SemanticAnalyzerDiagnostic::new(code, message.trim());
			Some(version.map_or(diagnostic.clone(), |version| {
				diagnostic.with_reference(format!(
					"https://github.com/obi1kenobi/cargo-semver-checks/tree/v{version}/src/lints/{code}.ron"
				))
			}))
		})
		.collect::<Vec<_>>();
	diagnostics.sort();
	diagnostics.dedup();
	diagnostics
}

fn failed_check(
	cell: &CargoSemverMatrixCell,
	configuration: BTreeMap<String, String>,
	reason: String,
) -> SemanticAnalyzerCheck {
	SemanticAnalyzerCheck::new(
		cell.name.clone(),
		SemanticAnalyzerCheckStatus::Failed,
		configuration,
	)
	.with_diagnostics(vec![SemanticAnalyzerDiagnostic::new(
		"analyzer_failed",
		reason,
	)])
}

fn aggregate_analysis(
	checks: Vec<SemanticAnalyzerCheck>,
	version: Option<String>,
) -> CargoSemverAnalysis {
	let checked = checks
		.iter()
		.filter(|check| check.status == SemanticAnalyzerCheckStatus::Checked)
		.count();
	let complete = !checks.is_empty() && checked == checks.len();
	let (outcome, bump) = checks
		.iter()
		.filter_map(|check| check.outcome.zip(check.suggested_bump))
		.max_by_key(|(_, bump)| *bump)
		.unwrap_or((SemanticAnalysisOutcome::Inconclusive, BumpSeverity::Patch));
	let fallback_reason = (!complete).then(|| {
		format!(
			"{} of {} configured matrix cells completed",
			checked,
			checks.len()
		)
	});
	let completeness = if complete {
		SemanticAnalysisCompleteness::Complete
	} else if checked == 0 {
		SemanticAnalysisCompleteness::Unsupported
	} else {
		SemanticAnalysisCompleteness::Partial
	};
	let mut evidence = SemanticAnalyzerEvidence::new(
		ANALYZER_ID,
		ENGINE,
		completeness,
		format!(
			"cargo-semver-checks active lints checked {checked}/{} configured feature/target cells; additive Rust exports remain syntax-derived",
			checks.len()
		),
	)
	.with_checks(checks);
	if let Some(version) = version {
		evidence = evidence.with_version(version);
	}
	if let Some(reason) = fallback_reason {
		evidence = evidence.with_fallback_reason(reason);
	}
	let confidence = if outcome == SemanticAnalysisOutcome::Breaking || complete {
		ApiConfidence::High
	} else {
		ApiConfidence::Low
	};
	let summary = match outcome {
		SemanticAnalysisOutcome::Breaking => {
			"cargo-semver-checks found a breaking Rust API change in the configured matrix"
		}
		SemanticAnalysisOutcome::Additive => {
			"cargo-semver-checks found an additive Rust API change in the configured matrix"
		}
		SemanticAnalysisOutcome::Compatible => {
			"cargo-semver-checks found no required version bump in the configured matrix"
		}
		SemanticAnalysisOutcome::Inconclusive => {
			"cargo-semver-checks could not complete the configured matrix"
		}
		// patch-coverage:ignore-start -- the enum is non-exhaustive for downstream compatibility; every current variant is tested.
		_ => "cargo-semver-checks returned an unknown compatibility outcome",
		// patch-coverage:ignore-end
	};
	CargoSemverAnalysis {
		change: SemanticChange::new(
			SemanticChangeCategory::PublicApi,
			SemanticChangeKind::Modified,
			"compatibility_matrix",
			"configured_matrix",
			summary,
			CARGO_MANIFEST_FILE,
		)
		.with_assessment(SemanticChangeAssessment::new(
			outcome, bump, confidence, evidence,
		)),
		replace_syntax_changes: complete && outcome != SemanticAnalysisOutcome::Breaking,
	}
}

fn failed_analysis(reason: &str) -> CargoSemverAnalysis {
	failed_analysis_with_version(reason, None)
}

fn failed_analysis_with_version(reason: &str, version: Option<String>) -> CargoSemverAnalysis {
	let check = SemanticAnalyzerCheck::new(
		"configured_matrix",
		SemanticAnalyzerCheckStatus::Skipped,
		BTreeMap::new(),
	)
	.with_diagnostics(vec![SemanticAnalyzerDiagnostic::new(
		"analyzer_unavailable",
		reason,
	)]);
	let mut evidence = SemanticAnalyzerEvidence::new(
		ANALYZER_ID,
		ENGINE,
		SemanticAnalysisCompleteness::Unsupported,
		"cargo-semver-checks did not check the configured feature/target matrix",
	)
	.with_fallback_reason(reason)
	.with_checks(vec![check]);
	if let Some(version) = version {
		evidence = evidence.with_version(version);
	}
	CargoSemverAnalysis {
		change: SemanticChange::new(
			SemanticChangeCategory::PublicApi,
			SemanticChangeKind::Modified,
			"compatibility_matrix",
			"configured_matrix",
			"cargo-semver-checks was unavailable; monochange retained conservative syntax evidence",
			CARGO_MANIFEST_FILE,
		)
		.with_assessment(SemanticChangeAssessment::new(
			SemanticAnalysisOutcome::Inconclusive,
			BumpSeverity::Patch,
			ApiConfidence::Low,
			evidence,
		)),
		replace_syntax_changes: false,
	}
}

fn concise_process_detail(stdout: &str, stderr: &str) -> String {
	stderr
		.lines()
		.chain(stdout.lines())
		.map(str::trim)
		.find(|line| !line.is_empty())
		.unwrap_or("no diagnostic output")
		.chars()
		.take(500)
		.collect()
}

#[cfg(test)]
#[path = "__tests__/semver_checks_tests.rs"]
mod tests;
