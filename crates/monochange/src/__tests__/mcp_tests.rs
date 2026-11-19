use std::fs;
use std::path::PathBuf;

use insta::assert_snapshot;
use monochange_analysis::ChangeAnalysis;
use monochange_analysis::ChangeFrame;
use monochange_analysis::DetectionLevel;
use monochange_analysis::PackageChangeAnalysis;
use monochange_config::LoadedChangesetFile;
use monochange_core::ChangeSignal;
use monochange_core::Ecosystem;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use monochange_test_helpers::content_text;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::git;
use monochange_test_helpers::snapshot_settings;
use rmcp::ServerHandler;
use rmcp::handler::server::wrapper::Parameters;
use tempfile::tempdir;

use super::AffectedParam;
use super::ChangeParam;
use super::ClassifyChangesParam;
use super::DiagnosticsParam;
use super::EmptyParam;
use super::LintExplainParam;
use super::McpChangeBump;
use super::MonochangeMcpServer;
use super::PathParam;
use super::json_error_result;
use super::json_result;
use super::parse_frame;
use super::resolve_root;
use super::resolve_root_with_current_dir;

macro_rules! assert_readable_json_snapshot {
	($value:expr) => {{
		let (redacted, multiline_fields) = redact_multiline_strings(&$value);
		insta::assert_json_snapshot!(redacted);

		for (path, contents) in multiline_fields {
			insta::assert_snapshot!(format!("multiline_{}", snapshot_path_slug(&path)), contents);
		}
	}};
}

fn redact_multiline_strings(
	value: &serde_json::Value,
) -> (serde_json::Value, Vec<(String, String)>) {
	let mut redacted = value.clone();
	let mut multiline_fields = Vec::new();
	redact_multiline_strings_at(&mut redacted, "$", &mut multiline_fields);
	(redacted, multiline_fields)
}

fn snapshot_path_slug(path: &str) -> String {
	path.chars()
		.map(|character| {
			match character {
				'a'..='z' | 'A'..='Z' | '0'..='9' => character.to_ascii_lowercase(),
				_ => '_',
			}
		})
		.collect::<String>()
		.trim_matches('_')
		.to_owned()
}

fn redact_multiline_strings_at(
	value: &mut serde_json::Value,
	path: &str,
	multiline_fields: &mut Vec<(String, String)>,
) {
	match value {
		serde_json::Value::String(contents) if contents.contains('\n') => {
			multiline_fields.push((path.to_owned(), contents.clone()));
			*contents = "[multiline text]".to_owned();
		}

		serde_json::Value::Array(items) => {
			for (index, item) in items.iter_mut().enumerate() {
				redact_multiline_strings_at(item, &format!("{path}[{index}]"), multiline_fields);
			}
		}
		serde_json::Value::Object(fields) => {
			for (key, field) in fields {
				redact_multiline_strings_at(field, &format!("{path}.{key}"), multiline_fields);
			}
		}
		_ => {}
	}
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn parse_frame_supports_named_range_and_pull_request_inputs() {
	assert_eq!(parse_frame("working"), ChangeFrame::WorkingDirectory);
	assert_eq!(
		parse_frame("main...feature/indexing"),
		ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature/indexing".to_string(),
		}
	);
	assert_eq!(
		parse_frame("pr:main,fix/indexing"),
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "fix/indexing".to_string(),
		}
	);
}

fn setup_analysis_workspace() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/before"),
		tempdir.path(),
	);
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/after"),
		tempdir.path(),
	);

	tempdir
}

fn setup_classification_workspace() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("api-classification/mixed-api/before"),
		tempdir.path(),
	);
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	copy_directory(
		&fixture_path("api-classification/mixed-api/after"),
		tempdir.path(),
	);

	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "api changes"]);
	tempdir
}

fn setup_analysis_workspace_without_git() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/after"),
		tempdir.path(),
	);
	tempdir
}

fn setup_analysis_workspace_without_commits() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(
		&fixture_path("analysis/cargo-public-api-diff/after"),
		tempdir.path(),
	);
	git(tempdir.path(), &["init"]);
	tempdir
}

fn sample_package_analysis(
	package_id: &str,
	semantic_changes: Vec<SemanticChange>,
) -> PackageChangeAnalysis {
	PackageChangeAnalysis {
		package_id: package_id.to_string(),
		package_record_id: package_id.to_string(),
		package_name: package_id.to_string(),
		ecosystem: Ecosystem::Cargo,
		release_identity: None,
		analyzer_id: Some("test".to_string()),
		changed_files: vec![PathBuf::from("src/lib.rs")],
		semantic_changes,
		warnings: Vec::new(),
	}
}

fn sample_change_analysis(package_analysis: PackageChangeAnalysis) -> ChangeAnalysis {
	let mut analyses_by_package = std::collections::BTreeMap::new();
	analyses_by_package.insert(package_analysis.package_id.clone(), package_analysis);
	ChangeAnalysis {
		frame: ChangeFrame::WorkingDirectory,
		detection_level: DetectionLevel::Signature,
		package_analyses: analyses_by_package,
		warnings: Vec::new(),
		packages: Vec::new(),
	}
}

fn sample_changeset(summary: Option<&str>, package_id: &str) -> LoadedChangesetFile {
	LoadedChangesetFile {
		path: PathBuf::from(".changeset/test.md"),
		summary: summary.map(ToString::to_string),
		details: None,
		targets: Vec::new(),
		signals: vec![ChangeSignal {
			package_id: package_id.to_string(),
			requested_bump: None,
			explicit_version: None,
			change_origin: "test".to_string(),
			evidence_refs: Vec::new(),
			notes: None,
			details: None,
			change_type: None,
			caused_by: Vec::new(),
			source_path: PathBuf::from(".changeset/test.md"),
		}],
	}
}

#[test]
fn get_info_exposes_tool_instructions_and_capabilities() {
	let info = MonochangeMcpServer::new().get_info();
	assert!(info.instructions.as_ref().is_some_and(|text| {
		text.contains(
			"monochange manages versions and releases across Cargo, npm, Deno, Dart/Flutter, Python, and Go workspaces",
		) && text.contains("classify_changes")
	}));
	assert!(info.capabilities.tools.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn classify_changes_matches_the_cli_report_with_public_dependency_propagation() {
	let fixture = setup_classification_workspace();
	let options = monochange_classification::ClassifyOptions {
		base: Some("HEAD~1".to_string()),
		head: "HEAD".to_string(),
		release: None,
		packages: Vec::new(),
		detection_level: DetectionLevel::Signature,
		include_unchanged: false,
		strict: false,
		skip_cli_snapshots: true,
		format: monochange_classification::ClassificationFormat::Json,
		output: None,
		labels: Vec::new(),
		dependency_propagation: monochange_classification::DependencyPropagation::Public,
	};
	let expected =
		monochange_classification::render_change_classification(fixture.path(), &options)
			.unwrap_or_else(|error| panic!("CLI classification: {error}"));
	let expected = serde_json::from_str::<serde_json::Value>(&expected)
		.unwrap_or_else(|error| panic!("parse CLI classification: {error}"));

	let result = MonochangeMcpServer::new()
		.classify_changes(Parameters(ClassifyChangesParam {
			path: Some(fixture.path().display().to_string()),
			base: Some("HEAD~1".to_string()),
			head: Some("HEAD".to_string()),
			release: None,
			packages: Vec::new(),
			detection_level: Some("signature".to_string()),
			include_unchanged: false,
			dependency_propagation: Some("public".to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("MCP classification: {error}"));
	let response = serde_json::from_str::<serde_json::Value>(&content_text(&result))
		.unwrap_or_else(|error| panic!("parse MCP classification: {error}"));

	assert_eq!(response["ok"], true);
	assert_eq!(response["report"], expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_reports_success_for_valid_workspace_fixture() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.validate(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn change_writes_markdown_changeset() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.change(Parameters(ChangeParam {
			path: Some(tempdir.path().display().to_string()),
			package: vec!["core".to_string()],
			bump: McpChangeBump::Patch,
			version: None,
			reason: "add test coverage".to_string(),
			change_type: None,
			caused_by: Vec::new(),
			details: None,
			output: Some(
				tempdir
					.path()
					.join(".changeset/core.md")
					.display()
					.to_string(),
			),
		}))
		.await
		.unwrap_or_else(|error| panic!("change: {error}"));

	assert_snapshot!("response", content_text(&result));
	let contents = fs::read_to_string(tempdir.path().join(".changeset/core.md"))
		.unwrap_or_else(|error| panic!("changeset read: {error}"));
	assert_snapshot!("changeset", contents);
}

#[tokio::test(flavor = "multi_thread")]
async fn discover_reports_config_errors_for_invalid_workspace_fixture() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("config/rejects-unknown-template-vars");

	let result = MonochangeMcpServer::new()
		.discover(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnostics_reports_missing_requested_changesets() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/diagnostics-base");

	let result = MonochangeMcpServer::new()
		.diagnostics(Parameters(DiagnosticsParam {
			path: Some(tempdir.path().display().to_string()),
			changeset: vec!["missing.md".to_string()],
		}))
		.await
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn change_reports_errors_for_unknown_package_ids() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.change(Parameters(ChangeParam {
			path: Some(tempdir.path().display().to_string()),
			package: vec!["missing".to_string()],
			bump: McpChangeBump::Patch,
			version: None,
			reason: "missing package".to_string(),
			change_type: None,
			caused_by: Vec::new(),
			details: None,
			output: None,
		}))
		.await
		.unwrap_or_else(|error| panic!("change: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_preview_returns_dry_run_release_summary() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/release-base");

	let result = MonochangeMcpServer::new()
		.release_preview(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release preview: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_preview_reports_config_errors_when_changesets_are_missing() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("cli-output/ungrouped-no-changeset");

	let result = MonochangeMcpServer::new()
		.release_preview(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release preview: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_manifest_reports_config_errors_when_changesets_are_missing() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("cli-output/ungrouped-no-changeset");

	let result = MonochangeMcpServer::new()
		.release_manifest(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_reports_success_for_documentation_only_changes() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("cli-step-input-overrides/workspace");

	let result = MonochangeMcpServer::new()
		.affected_packages(Parameters(AffectedParam {
			path: Some(tempdir.path().display().to_string()),
			changed_paths: vec!["docs/readme.md".to_string()],
			labels: vec!["no-changeset-required".to_string()],
		}))
		.await
		.unwrap_or_else(|error| panic!("affected: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_reports_configuration_errors() {
	let tempdir = setup_fixture("config/rejects-empty-step-id");
	let result = MonochangeMcpServer::new()
		.affected_packages(Parameters(AffectedParam {
			path: Some(tempdir.path().display().to_string()),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			labels: Vec::new(),
		}))
		.await
		.unwrap_or_else(|error| panic!("affected: {error}"));
	let rendered = content_text(&result);
	assert!(rendered.contains("\"ok\": false"));
	assert!(
		rendered.contains("unknown") || rendered.contains("empty id") || rendered.contains("step")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn lint_catalog_lists_rules_and_presets() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let result = MonochangeMcpServer::new()
		.lint_catalog(Parameters(EmptyParam::default()))
		.await
		.unwrap_or_else(|error| panic!("lint catalog: {error}"));

	assert_snapshot!(content_text(&result));
}

#[tokio::test(flavor = "multi_thread")]
async fn lint_explain_returns_rule_and_preset() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let server = MonochangeMcpServer::new();
	let rule = server
		.lint_explain(Parameters(LintExplainParam {
			id: "cargo/internal-dependency-workspace".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("lint explain rule: {error}"));
	let preset = server
		.lint_explain(Parameters(LintExplainParam {
			id: "cargo/recommended".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("lint explain preset: {error}"));

	let combined = serde_json::json!({
		"rule": serde_json::from_str::<serde_json::Value>(&content_text(&rule))
			.unwrap_or_else(|error| panic!("parse rule result: {error}")),
		"preset": serde_json::from_str::<serde_json::Value>(&content_text(&preset))
			.unwrap_or_else(|error| panic!("parse preset result: {error}")),
	});

	assert_readable_json_snapshot!(combined);
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_eval_release_workflow_stays_machine_readable() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();
	let tempdir = setup_fixture("monochange/diagnostics-base");
	let server = MonochangeMcpServer::new();

	let discover = server
		.discover(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("discover: {error}"));
	let diagnostics = server
		.diagnostics(Parameters(DiagnosticsParam {
			path: Some(tempdir.path().display().to_string()),
			changeset: Vec::new(),
		}))
		.await
		.unwrap_or_else(|error| panic!("diagnostics: {error}"));
	let manifest = server
		.release_manifest(Parameters(PathParam {
			path: Some(tempdir.path().display().to_string()),
		}))
		.await
		.unwrap_or_else(|error| panic!("release manifest: {error}"));

	let eval = serde_json::json!({
		"discover": serde_json::from_str::<serde_json::Value>(&content_text(&discover))
			.unwrap_or_else(|error| panic!("parse discover result: {error}")),
		"diagnostics": serde_json::from_str::<serde_json::Value>(&content_text(&diagnostics))
			.unwrap_or_else(|error| panic!("parse diagnostics result: {error}")),
		"release_manifest": serde_json::from_str::<serde_json::Value>(&content_text(&manifest))
			.unwrap_or_else(|error| panic!("parse release manifest result: {error}")),
	});

	assert_readable_json_snapshot!(eval);
}

#[tokio::test(flavor = "multi_thread")]
async fn analyze_changes_returns_semantic_diff_for_cargo_workspace() {
	let tempdir = setup_analysis_workspace();
	let result = MonochangeMcpServer::new()
		.analyze_changes(Parameters(super::AnalyzeChangesParam {
			path: Some(tempdir.path().display().to_string()),
			frame: Some("working".to_string()),
			detection_level: Some("signature".to_string()),
			max_suggestions: None,
		}))
		.await
		.unwrap_or_else(|error| panic!("analyze_changes: {error}"));

	snapshot_settings().bind(|| {
		assert_snapshot!(content_text(&result));
	});
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_stale_symbol_references() {
	let tempdir = setup_analysis_workspace();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::copy(
		fixture_path("analysis/cargo-public-api-diff/changeset-stale.md"),
		changeset_dir.join("feature.md"),
	)
	.unwrap_or_else(|error| panic!("copy changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));

	snapshot_settings().bind(|| {
		assert_snapshot!(content_text(&result));
	});
}

#[test]
fn validate_changeset_content_reports_missing_package_diffs() {
	let changeset = sample_changeset(Some("mention `greet`"), "core");
	let analysis = ChangeAnalysis {
		frame: ChangeFrame::WorkingDirectory,
		detection_level: DetectionLevel::Signature,
		package_analyses: std::collections::BTreeMap::new(),
		warnings: Vec::new(),
		packages: Vec::new(),
	};

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert_eq!(issues.len(), 2);
	assert!(
		issues
			.iter()
			.any(|issue| issue.message.contains("has no current diff"))
	);
	assert!(
		issues
			.iter()
			.any(|issue| issue.message.contains("references `greet`"))
	);
}

#[test]
fn validate_changeset_content_warns_when_semantic_changes_are_empty() {
	let changeset = sample_changeset(Some("internal cleanup"), "core");
	let analysis = sample_change_analysis(sample_package_analysis("core", Vec::new()));

	let issues = super::validate_changeset_content(&changeset, &analysis);

	assert_eq!(issues.len(), 1);
	assert!(
		issues[0]
			.message
			.contains("no semantic changes were detected")
	);
	assert_eq!(issues[0].severity, "warning");
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_frame_detection_errors() {
	let tempdir = setup_analysis_workspace_without_git();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(
		changeset_dir.join("feature.md"),
		"---\ncore: patch\n---\n\n#### test change\n",
	)
	.unwrap_or_else(|error| panic!("write changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));
	let rendered = content_text(&result);

	assert!(rendered.contains("Failed to detect change frame"));
}

#[tokio::test(flavor = "multi_thread")]
async fn validate_changeset_reports_semantic_analysis_errors() {
	let tempdir = setup_analysis_workspace_without_commits();
	let changeset_dir = tempdir.path().join(".changeset");
	fs::create_dir_all(&changeset_dir)
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(
		changeset_dir.join("feature.md"),
		"---\ncore: patch\n---\n\n#### test change\n",
	)
	.unwrap_or_else(|error| panic!("write changeset fixture: {error}"));

	let result = MonochangeMcpServer::new()
		.validate_changeset(Parameters(super::ValidateChangesetParam {
			path: Some(tempdir.path().display().to_string()),
			changeset_path: ".changeset/feature.md".to_string(),
		}))
		.await
		.unwrap_or_else(|error| panic!("validate_changeset: {error}"));
	let rendered = content_text(&result);

	assert!(rendered.contains("Semantic analysis failed"));
}
