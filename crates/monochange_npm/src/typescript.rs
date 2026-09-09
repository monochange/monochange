use std::ffi::OsStr;
use std::fs;
use std::io::Read as _;
use std::io::Write as _;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;

use monochange_core::ApiConfidence;
use monochange_core::BumpSeverity;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageSnapshot;
use monochange_core::SemanticAnalysisCompleteness;
use monochange_core::SemanticAnalysisOutcome;
use monochange_core::SemanticAnalyzerEvidence;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeAssessment;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use monochange_core::normalize_path;
use serde::Deserialize;
use serde::Serialize;
use wait_timeout::ChildExt as _;

const ANALYZER_ID: &str = "npm/typescript";
const ENGINE: &str = "typescript";
const HELPER: &str = include_str!("typescript_analyzer.mjs");
const MAX_PROCESS_OUTPUT_BYTES: u64 = 8 * 1024 * 1024;
const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub(super) struct TypeScriptAnalysis {
	pub(super) changes: Vec<SemanticChange>,
	pub(super) warnings: Vec<String>,
}

pub(super) fn has_typescript_surface(context: &PackageAnalysisContext<'_>) -> bool {
	context
		.before_snapshot
		.into_iter()
		.chain(context.after_snapshot)
		.any(snapshot_has_typed_surface)
}

fn snapshot_has_typed_surface(snapshot: &PackageSnapshot) -> bool {
	let Some(manifest) = snapshot.file(Path::new("package.json")) else {
		return snapshot.file(Path::new("index.d.ts")).is_some();
	};
	let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&manifest.contents) else {
		return false;
	};

	manifest
		.get("types")
		.or_else(|| manifest.get("typings"))
		.is_some_and(serde_json::Value::is_string)
		|| manifest
			.get("exports")
			.is_some_and(|value| export_is_typed(value, snapshot))
		|| ["main", "module"]
			.iter()
			.filter_map(|field| manifest.get(field).and_then(serde_json::Value::as_str))
			.any(|target| target_has_declaration(target, snapshot))
		|| snapshot.file(Path::new("index.d.ts")).is_some()
}

fn export_is_typed(value: &serde_json::Value, snapshot: &PackageSnapshot) -> bool {
	match value {
		serde_json::Value::String(target) => target_has_declaration(target, snapshot),
		serde_json::Value::Array(values) => {
			values.iter().any(|value| export_is_typed(value, snapshot))
		}
		serde_json::Value::Object(entries) => {
			entries.contains_key("types")
				|| entries
					.values()
					.any(|value| export_is_typed(value, snapshot))
		}
		_ => false,
	}
}

fn target_has_declaration(target: &str, snapshot: &PackageSnapshot) -> bool {
	if is_typescript_target(target) {
		return true;
	}

	let target = target.strip_prefix("./").unwrap_or(target);
	let path = Path::new(target);
	let Some(extension) = path.extension().and_then(OsStr::to_str) else {
		return false;
	};
	if !matches!(extension, "js" | "jsx" | "mjs" | "cjs") {
		return false;
	}

	let stem = target.strip_suffix(extension).unwrap_or(target);
	["d.ts", "d.mts", "d.cts"]
		.iter()
		.any(|declaration_extension| {
			snapshot
				.file(Path::new(&format!("{stem}{declaration_extension}")))
				.is_some()
		})
}

fn is_typescript_target(target: &str) -> bool {
	[".ts", ".tsx", ".mts", ".cts"]
		.iter()
		.any(|extension| target.ends_with(extension))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TypeScriptRequest<'a> {
	repo_root: &'a Path,
	package_root: &'a Path,
	package_name: &'a str,
	before: &'a PackageSnapshot,
	after: &'a PackageSnapshot,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeScriptResponse {
	version: Option<String>,
	fallback: bool,
	coverage: String,
	fallback_reason: Option<String>,
	warnings: Vec<String>,
	changes: Vec<TypeScriptChange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TypeScriptChange {
	outcome: SemanticAnalysisOutcome,
	suggested_bump: BumpSeverity,
	kind: SemanticChangeKind,
	item_kind: String,
	item_path: String,
	summary: String,
	file_path: PathBuf,
	before_signature: Option<String>,
	after_signature: Option<String>,
	confidence: ApiConfidence,
	completeness: SemanticAnalysisCompleteness,
	coverage: String,
	fallback_reason: Option<String>,
}

pub(super) fn analyze_typescript(
	context: &PackageAnalysisContext<'_>,
	syntax_changes: Vec<SemanticChange>,
) -> TypeScriptAnalysis {
	analyze_typescript_with(context, syntax_changes, run_typescript)
}

fn analyze_typescript_with(
	context: &PackageAnalysisContext<'_>,
	syntax_changes: Vec<SemanticChange>,
	run: impl FnOnce(&TypeScriptRequest<'_>) -> Result<TypeScriptResponse, String>,
) -> TypeScriptAnalysis {
	let Some(before) = context.before_snapshot else {
		return fallback_analysis(
			syntax_changes,
			None,
			"the before package snapshot was unavailable".to_string(),
		);
	};
	let Some(after) = context.after_snapshot else {
		return fallback_analysis(
			syntax_changes,
			None,
			"the after package snapshot was unavailable".to_string(),
		);
	};
	let repo_root = normalize_path(context.repo_root);
	let package_root = context
		.package_root()
		.strip_prefix(context.repo_root)
		.or_else(|_| context.package_root().strip_prefix(&repo_root));
	let Ok(package_root) = package_root else {
		return fallback_analysis(
			syntax_changes,
			None,
			"the npm package root is outside the analyzed repository".to_string(),
		);
	};
	if (!package_root.as_os_str().is_empty() && !is_safe_relative_path(package_root))
		|| !snapshot_paths_are_safe(before)
		|| !snapshot_paths_are_safe(after)
	{
		return fallback_analysis(
			syntax_changes,
			None,
			"the TypeScript package snapshot contained an unsafe path".to_string(),
		);
	}
	let request = TypeScriptRequest {
		repo_root: &repo_root,
		package_root,
		package_name: &context.package.name,
		before,
		after,
	};

	match run(&request) {
		Ok(response) if response.fallback => {
			fallback_analysis(
				syntax_changes,
				response.version,
				response.fallback_reason.unwrap_or(response.coverage),
			)
		}
		Ok(response) => response_analysis(response, syntax_changes),
		Err(reason) => fallback_analysis(syntax_changes, None, reason),
	}
}

fn run_typescript(request: &TypeScriptRequest<'_>) -> Result<TypeScriptResponse, String> {
	run_typescript_with_node(request, OsStr::new("node"))
}

fn run_typescript_with_node(
	request: &TypeScriptRequest<'_>,
	node_executable: &OsStr,
) -> Result<TypeScriptResponse, String> {
	run_typescript_process(
		request,
		node_executable,
		HELPER,
		PROCESS_TIMEOUT,
		MAX_PROCESS_OUTPUT_BYTES,
		MAX_REQUEST_BYTES,
	)
}

fn run_typescript_process(
	request: &TypeScriptRequest<'_>,
	node_executable: &OsStr,
	helper: &str,
	timeout: Duration,
	max_output_bytes: u64,
	max_request_bytes: usize,
) -> Result<TypeScriptResponse, String> {
	let request_body = serde_json::to_vec(request)
		.map_err(|error| format!("failed to serialize TypeScript analyzer input: {error}"))?;
	if request_body.len() > max_request_bytes {
		return Err(format!(
			"TypeScript analyzer input exceeded its {max_request_bytes} byte limit"
		));
	}
	let temp_dir = tempfile::Builder::new()
		.prefix("monochange-typescript-")
		.tempdir()
		.map_err(|error| format!("failed to create TypeScript analyzer directory: {error}"))?;
	let helper_path = temp_dir.path().join("analyzer.mjs");
	fs::write(&helper_path, helper)
		.map_err(|error| format!("failed to write TypeScript analyzer helper: {error}"))?;
	let mut child = Command::new(node_executable)
		.arg(&helper_path)
		.current_dir(request.repo_root)
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.map_err(|error| format!("failed to start Node for TypeScript analysis: {error}"))?;
	let stdout = child.stdout.take().expect("stdout was configured as piped");
	let stderr = child.stderr.take().expect("stderr was configured as piped");
	let stdout_reader = std::thread::spawn(move || read_bounded(stdout, max_output_bytes));
	let stderr_reader = std::thread::spawn(move || read_bounded(stderr, max_output_bytes));
	let mut stdin = child.stdin.take().expect("stdin was configured as piped");
	if let Err(error) = stdin.write_all(&request_body) {
		let _ = child.kill();
		let _ = child.wait();
		let _ = join_reader(stdout_reader, "stdout");
		let _ = join_reader(stderr_reader, "stderr");

		return Err(format!("failed to send TypeScript analyzer input: {error}"));
	}
	drop(stdin);

	// patch-coverage:ignore-start -- wait_timeout only errors when the OS wait primitive fails;
	// success, process failure, and timeout are exercised with real child processes.
	let Some(status) = child
		.wait_timeout(timeout)
		.map_err(|error| format!("failed to wait for TypeScript analyzer: {error}"))?
	else {
		// patch-coverage:ignore-end
		let _ = child.kill();
		let _ = child.wait();
		let _ = join_reader(stdout_reader, "stdout");
		let _ = join_reader(stderr_reader, "stderr");

		return Err(format!(
			"TypeScript analysis exceeded its {} second limit",
			timeout.as_secs_f64()
		));
	};
	let stdout = join_reader(stdout_reader, "stdout")?;
	let stderr = join_reader(stderr_reader, "stderr")?;

	if !status.success() {
		return Err(format!(
			"TypeScript analyzer exited with {status}: {}",
			String::from_utf8_lossy(&stderr).trim()
		));
	}

	serde_json::from_slice(&stdout).map_err(|error| {
		format!(
			"failed to parse TypeScript analyzer output: {error}; stderr: {}",
			String::from_utf8_lossy(&stderr).trim()
		)
	})
}

fn read_bounded(reader: impl std::io::Read, max_bytes: u64) -> std::io::Result<Vec<u8>> {
	let mut output = Vec::new();
	reader.take(max_bytes + 1).read_to_end(&mut output)?;

	if output.len() as u64 > max_bytes {
		return Err(std::io::Error::other(format!(
			"TypeScript analyzer output exceeded its {max_bytes} byte limit"
		)));
	}

	Ok(output)
}

fn join_reader(
	reader: std::thread::JoinHandle<std::io::Result<Vec<u8>>>,
	stream: &str,
) -> Result<Vec<u8>, String> {
	reader
		.join()
		.map_err(|_| format!("TypeScript analyzer {stream} reader panicked"))?
		.map_err(|error| format!("failed to read TypeScript analyzer {stream}: {error}"))
}

fn response_analysis(
	response: TypeScriptResponse,
	syntax_changes: Vec<SemanticChange>,
) -> TypeScriptAnalysis {
	let mut warnings = response.warnings;
	let mut changes = Vec::with_capacity(response.changes.len());

	for change in response.changes {
		if !is_safe_relative_path(&change.file_path) {
			return fallback_analysis(
				syntax_changes,
				response.version,
				format!(
					"TypeScript analyzer returned unsafe evidence path `{}`",
					change.file_path.display()
				),
			);
		}

		let evidence = analyzer_evidence(
			response.version.clone(),
			change.completeness,
			change.coverage,
			change.fallback_reason,
		);
		let assessment = SemanticChangeAssessment::new(
			change.outcome,
			change.suggested_bump,
			change.confidence,
			evidence,
		);
		let mut semantic_change = SemanticChange::new(
			SemanticChangeCategory::PublicApi,
			change.kind,
			change.item_kind,
			change.item_path,
			change.summary,
			change.file_path,
		);
		semantic_change.before_signature = change.before_signature;
		semantic_change.after_signature = change.after_signature;
		semantic_change.assessment = Some(assessment);
		changes.push(semantic_change);
	}

	if let Some(reason) = response.fallback_reason {
		warnings.push(reason);
	}
	warnings.sort();
	warnings.dedup();

	TypeScriptAnalysis { changes, warnings }
}

fn fallback_analysis(
	mut syntax_changes: Vec<SemanticChange>,
	version: Option<String>,
	reason: String,
) -> TypeScriptAnalysis {
	let evidence = analyzer_evidence(
		version,
		SemanticAnalysisCompleteness::Partial,
		"syntax fallback; TypeScript declaration compatibility was not completed".to_string(),
		Some(reason.clone()),
	);

	if syntax_changes.is_empty() {
		syntax_changes.push(
			SemanticChange::new(
				SemanticChangeCategory::PublicApi,
				SemanticChangeKind::Modified,
				"declaration_analysis",
				"typescript",
				"TypeScript declaration compatibility is inconclusive",
				PathBuf::from("package.json"),
			)
			.with_assessment(inconclusive_assessment(evidence.clone())),
		);
	} else {
		for change in &mut syntax_changes {
			change.assessment = Some(inconclusive_assessment(evidence.clone()));
		}
	}

	TypeScriptAnalysis {
		changes: syntax_changes,
		warnings: vec![reason],
	}
}

fn inconclusive_assessment(evidence: SemanticAnalyzerEvidence) -> SemanticChangeAssessment {
	SemanticChangeAssessment::new(
		SemanticAnalysisOutcome::Inconclusive,
		BumpSeverity::Patch,
		ApiConfidence::Low,
		evidence,
	)
}

fn analyzer_evidence(
	version: Option<String>,
	completeness: SemanticAnalysisCompleteness,
	coverage: String,
	fallback_reason: Option<String>,
) -> SemanticAnalyzerEvidence {
	let mut evidence = SemanticAnalyzerEvidence::new(ANALYZER_ID, ENGINE, completeness, coverage);
	evidence.version = version;
	evidence.fallback_reason = fallback_reason;
	evidence
}

fn is_safe_relative_path(path: &Path) -> bool {
	let rendered = path.to_string_lossy();
	!rendered.is_empty()
		&& !rendered.contains('\\')
		&& rendered.as_bytes().get(1).is_none_or(|byte| *byte != b':')
		&& !path.is_absolute()
		&& path
			.components()
			.all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn snapshot_paths_are_safe(snapshot: &PackageSnapshot) -> bool {
	snapshot
		.files
		.iter()
		.all(|file| is_safe_relative_path(&file.path))
}

#[cfg(test)]
#[path = "__tests__/typescript_tests.rs"]
mod tests;
