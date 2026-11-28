#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]
#![allow(unused_imports, unused_qualifications)]
use std::cell::Cell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use clap::Command;
use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::MockServer;
use monochange_changelog::ReleaseNoteChange;
use monochange_changelog::filter_group_release_note_change;
use monochange_changelog::group_changelog_include_allows;
use monochange_changelog::render_group_filtered_update_message;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangesetTargetKind;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::Ecosystem;
use monochange_core::GroupChangelogInclude;
use monochange_core::PreparedChangesetTarget;
use monochange_core::VersionFormat;
use monochange_core::VersionSource;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::snapshot_settings;
use semver::Version;
use tempfile::tempdir;

use crate::CliContext;
use crate::OutputFormat;
use crate::PreparedFileDiff;
use crate::SnapshotRequest;
use crate::add_change_file;
use crate::add_interactive_change_file;
use crate::affected_packages;
use crate::cli::apply_runtime_change_type_choices;
use crate::cli::apply_runtime_prepare_release_output_formats;
use crate::cli::build_command_for_root;
use crate::cli::build_skill_subcommand;
use crate::cli::build_subagents_subcommand;
use crate::cli::cli_command_after_help;
use crate::cli::cli_commands_for_root;
use crate::cli::configured_change_type_choices;
use crate::cli_runtime::build_cli_template_context;
use crate::cli_runtime::build_retarget_release_report;
use crate::cli_runtime::inferred_retarget_source_configuration;
use crate::cli_runtime::lookup_template_value;
use crate::cli_runtime::normalize_when_expression;
use crate::cli_runtime::parse_boolean_step_input;
use crate::cli_runtime::parse_change_bump;
use crate::cli_runtime::parse_direct_template_reference;
use crate::cli_runtime::render_cli_command_markdown_result;
use crate::cli_runtime::render_cli_command_result;
use crate::cli_runtime::render_retarget_release_report;
use crate::cli_runtime::retarget_operation_label;
use crate::cli_runtime::should_execute_cli_step;
use crate::cli_runtime::template_value_to_input_values;
use crate::discover_workspace;
use crate::git_support::read_git_commit_message;
use crate::git_support::run_git_capture;
use crate::git_support::run_git_process;
use crate::git_support::run_git_status;
use crate::interactive::InteractiveChangeResult;
use crate::interactive::InteractiveTarget;
use crate::missing_configuration_warning;
use crate::parse_snapshot_request;
use crate::plan_release;
use crate::push_change_target_markdown;
use crate::release_artifacts::validate_release_record_file;
use crate::release_artifacts::write_release_record_file;
use crate::render_cli_snapshot_classification;
use crate::render_snapshot_request;
use crate::run_with_args;
use crate::run_with_args_in_dir;
use crate::snapshot_view;
use crate::tests::TEST_ENV_LOCK;
use crate::workspace_ops::build_lockfile_command_executions;
use crate::workspace_ops::change_type_default_bump;
use crate::workspace_ops::prepare_release_execution_with_file_diffs;
use crate::workspace_ops::render_interactive_changeset_markdown;

async fn execute_cli_command(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command: &CliCommandDefinition,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
) -> monochange_core::MonochangeResult<String> {
	crate::cli_runtime::execute_cli_command_with_options(
		root,
		configuration,
		cli_command,
		crate::cli_runtime::ExecuteCliCommandOptions {
			dry_run,
			quiet: false,
			show_diff: false,
			inputs,
			prepared_release_path: None,
			progress_format: crate::output::ProgressFormat::Auto,
			progress: None,
		},
	)
	.await
}

async fn execute_matches(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command_name: &str,
	cli_command_matches: &clap::ArgMatches,
	quiet: bool,
) -> monochange_core::MonochangeResult<String> {
	crate::cli_runtime::execute_matches_with_progress(
		root,
		configuration,
		cli_command_name,
		cli_command_matches,
		quiet,
		None,
	)
	.await
}

fn apply_versioned_file_definition(
	root: &Path,
	updates: &mut BTreeMap<PathBuf, crate::CachedDocument>,
	definition: &monochange_core::VersionedFileDefinition,
	owner_version: &str,
	shared_release_version: Option<&String>,
	dep_names: &[impl AsRef<str>],
	context: &crate::VersionedFileUpdateContext<'_>,
) -> monochange_core::MonochangeResult<()> {
	let resolved_paths = monochange_core::workspace_glob_files(root, &definition.path)?;
	crate::apply_versioned_file_definition_to_paths(
		root,
		updates,
		definition,
		&super::VersionedFileSite {
			resolved_paths: &resolved_paths,
			owner_version,
			shared_release_version,
			dep_names,
		},
		context,
	)
}

fn render_change_target_markdown(
	configuration: &monochange_core::WorkspaceConfiguration,
	target_id: &str,
	bump: BumpSeverity,
	version: Option<&str>,
	change_type: Option<&str>,
	caused_by: &[String],
) -> monochange_core::MonochangeResult<Vec<String>> {
	let mut rendered = String::new();
	push_change_target_markdown(
		&mut rendered,
		configuration,
		target_id,
		bump,
		version,
		change_type,
		caused_by,
	)?;
	Ok(rendered.lines().map(str::to_string).collect())
}

#[allow(unused_macro_rules)]
macro_rules! assert_readable_json_snapshot {
	($value:expr) => {{
		let (redacted, multiline_fields) = redact_multiline_strings(&$value);
		insta::assert_json_snapshot!(redacted);

		for (path, contents) in multiline_fields {
			insta::assert_snapshot!(format!("multiline_{}", snapshot_path_slug(&path)), contents);
		}
	}};
	($name:expr, $value:expr) => {{
		let (redacted, multiline_fields) = redact_multiline_strings(&$value);
		insta::assert_json_snapshot!($name, redacted);

		for (path, contents) in multiline_fields {
			insta::assert_snapshot!(
				format!("{}_multiline_{}", $name, snapshot_path_slug(&path)),
				contents
			);
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

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn run_cli<I>(root: &Path, args: I) -> monochange_core::MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let mut args = args.into_iter().collect::<Vec<_>>();
	let command_index = args.iter().enumerate().skip(1).find_map(|(index, value)| {
		value
			.to_str()
			.filter(|arg| !arg.starts_with('-'))
			.map(|_| index)
	});
	if let Some((command_index, command)) = command_index.and_then(|index| {
		args.get(index)
			.and_then(|value| value.to_str())
			.map(|command| (index, command))
	}) {
		let builtins = [
			"affected",
			"analyze",
			"api",
			"audit",
			"change",
			"changeset",
			"check",
			"command",
			"completions",
			"config",
			"doctor",
			"create",
			"diagnose",
			"discover",
			"explain",
			"help",
			"init",
			"lint",
			"list",
			"mcp",
			"migrate",
			"new",
			"prepare",
			"preview",
			"release-records",
			"run",
			"skill",
			"step",
			"subagents",
			"versions",
			"--help",
			"--version",
		];

		if !builtins.contains(&command) {
			args.insert(command_index, OsString::from("run"));
		}
	}

	crate::tests::block_on_in_context(temp_env::async_with_vars(
		[("MONOCHANGE_PROGRESS_FORMAT", None::<&str>)],
		async { run_with_args_in_dir("monochange", args, root).await },
	))
}

#[test]
fn migrate_audit_reports_legacy_release_tooling() {
	let tempdir = setup_fixture("migration-audit/legacy-release-workflow");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit: {error}"));

	assert!(
		output
			.lines()
			.last()
			.unwrap_or_default()
			.starts_with("▲ Migration needed"),
		"{output}"
	);
	assert!(
		has_table_row(&output, &["knope", "legacy-release-tool", "knope.toml"]),
		"{output}"
	);
	assert!(
		has_table_row(
			&output,
			&["changesets", "ci-workflow", ".github/workflows/release.yml"]
		),
		"{output}"
	);
	assert!(output.contains("Plan trusted publishing per package"));

	let json_output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit json: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&json_output)
		.unwrap_or_else(|error| panic!("parse migration audit json: {error}"));

	assert_eq!(parsed["status"], serde_json::json!("migration-needed"));
	assert!(parsed["signals"].as_array().is_some_and(|signals| {
		signals.iter().any(|signal| {
			signal["tool"] == serde_json::json!("knope")
				&& signal["path"] == serde_json::json!("knope.toml")
		})
	}));
	assert!(parsed["recommendations"].as_array().is_some_and(|items| {
		items
			.iter()
			.any(|item| item["id"] == serde_json::json!("audit-changelogs"))
	}));
}

#[test]
fn migrate_release_records_rewrites_legacy_records() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("create tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	let record_path = root.join(".monochange/releases/abc123/release.json");
	fs::create_dir_all(record_path.parent().expect("record has parent"))
		.unwrap_or_else(|error| panic!("create release record dir: {error}"));
	fs::write(
		&record_path,
		r#"{
  "schema_version": 1,
  "kind": "monochange.releaseRecord",
  "created_at": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "release_targets": [],
  "changed_files": []
}"#,
	)
	.unwrap_or_else(|error| panic!("write legacy release record: {error}"));

	let output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("release-records"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("migrate release records: {error}"));
	let parsed: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("parse migration report: {error}"));

	assert_eq!(parsed["scanned"], serde_json::json!(1));
	assert_eq!(parsed["migrated"], serde_json::json!(1));
	assert_eq!(
		parsed["records"][0]["status"],
		serde_json::json!("migrated")
	);
	assert_eq!(
		parsed["records"][0]["from_schema_version"],
		serde_json::json!("legacy-1")
	);

	let migrated = fs::read_to_string(&record_path)
		.unwrap_or_else(|error| panic!("read migrated release record: {error}"));
	assert!(migrated.contains(&format!(
		r#""schema_version": "{}""#,
		monochange_core::RELEASE_RECORD_SCHEMA_VERSION
	)));
	assert!(!migrated.contains(r#""schema_version": 1"#));
}

#[test]
fn migrate_release_records_reports_dry_run_and_current_records() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("create tempdir: {error}"));
	let root = tempdir.path();
	let empty_report = crate::migration_audit::migrate_release_records(root, true)
		.unwrap_or_else(|error| panic!("migrate empty records: {error}"));
	assert_eq!(empty_report.scanned, 0);
	assert_eq!(empty_report.migrated, 0);
	assert_eq!(empty_report.unchanged, 0);

	write_blank_monochange_config(root);
	let current_record_path = root.join(".monochange/releases/current/release.json");
	let legacy_record_path = root.join(".monochange/releases/legacy/release.json");
	fs::create_dir_all(
		current_record_path
			.parent()
			.expect("current record has parent"),
	)
	.unwrap_or_else(|error| panic!("create current record dir: {error}"));
	fs::create_dir_all(
		legacy_record_path
			.parent()
			.expect("legacy record has parent"),
	)
	.unwrap_or_else(|error| panic!("create legacy record dir: {error}"));
	fs::write(
		&current_record_path,
		format!(
			r#"{{
  "schema_version": "{}",
  "kind": "monochange.releaseRecord",
  "created_at": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "release_targets": [],
  "changed_files": []
}}"#,
			monochange_core::RELEASE_RECORD_SCHEMA_VERSION
		),
	)
	.unwrap_or_else(|error| panic!("write current release record: {error}"));
	fs::write(
		&legacy_record_path,
		r#"{
  "v": "0.0",
  "kind": "monochange.releaseRecord",
  "created_at": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "release_targets": [],
  "released_packages": [],
  "changed_files": []
}"#,
	)
	.unwrap_or_else(|error| panic!("write legacy release record: {error}"));

	let output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("release-records"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("markdown"),
		],
	)
	.unwrap_or_else(|error| panic!("dry-run release-record migration: {error}"));

	assert!(
		output.contains("• Would migrate 1 of 2 release records to schema"),
		"{output}"
	);
	assert!(output.contains("Records (2)"), "{output}");
	assert!(output.contains("would migrate"));
	assert!(output.contains("current"));
	let legacy_after = fs::read_to_string(&legacy_record_path)
		.unwrap_or_else(|error| panic!("read dry-run legacy release record: {error}"));
	assert!(legacy_after.contains(r#""v": "0.0""#));
}

#[test]
fn migrate_audit_reports_ready_workspace_and_quiet_mode() {
	let tempdir = setup_fixture("migration-audit/ready-monochange");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
			OsString::from("--format"),
			OsString::from("markdown"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit ready: {error}"));

	assert!(
		output
			.lines()
			.last()
			.unwrap_or_default()
			.starts_with("✔ No migration needed"),
		"{output}"
	);
	assert!(
		has_table_row(
			&output,
			&["monochange", "monochange-config", "monochange.toml"]
		),
		"{output}"
	);
	assert!(!output.contains("Recommendations ("), "{output}");

	let quiet_output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("--quiet"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit quiet: {error}"));
	assert_eq!(quiet_output, "");

	let empty_tempdir = setup_fixture("migration-audit/empty-workspace");
	let empty_output = run_cli(
		empty_tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_or_else(|error| panic!("migration audit empty: {error}"));
	assert!(!empty_output.contains("Signals ("), "{empty_output}");
	assert!(empty_output.contains("Generate monochange configuration"));
}

/// Whether a text table has a row that starts with `cells`, ignoring the
/// column padding.
fn has_table_row(output: &str, cells: &[&str]) -> bool {
	output.lines().any(|line| {
		line.split_whitespace()
			.collect::<Vec<_>>()
			.starts_with(cells)
	})
}

#[test]
fn migrate_audit_reports_fixture_read_errors() {
	let package_json_dir = setup_fixture("migration-audit/unreadable-package-json");
	fs::rename(
		package_json_dir.path().join("package-json-dir"),
		package_json_dir.path().join("package.json"),
	)
	.unwrap_or_else(|error| panic!("rename package.json directory fixture: {error}"));
	let package_error = run_cli(
		package_json_dir.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_err();
	assert!(package_error.to_string().contains("failed to read"));
	assert!(package_error.to_string().contains("package.json"));

	let workflow_root_file = setup_fixture("migration-audit/unreadable-workflow-root");
	fs::rename(
		workflow_root_file.path().join(".github/workflows-file"),
		workflow_root_file.path().join(".github/workflows"),
	)
	.unwrap_or_else(|error| panic!("rename workflows file fixture: {error}"));
	let workflow_root_error = run_cli(
		workflow_root_file.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_err();
	assert!(
		workflow_root_error
			.to_string()
			.contains(".github/workflows")
	);

	let workflow_file_dir = setup_fixture("migration-audit/unreadable-workflow-file");
	fs::rename(
		workflow_file_dir
			.path()
			.join(".github/workflows/release-yml-dir"),
		workflow_file_dir
			.path()
			.join(".github/workflows/release.yml"),
	)
	.unwrap_or_else(|error| panic!("rename workflow directory fixture: {error}"));
	let workflow_file_error = run_cli(
		workflow_file_dir.path(),
		[
			OsString::from("monochange"),
			OsString::from("migrate"),
			OsString::from("audit"),
		],
	)
	.unwrap_err();
	assert!(workflow_file_error.to_string().contains("release.yml"));
}

#[test]
fn verify_release_branch_step_reports_matching_branch() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"[source]
provider = "github"
owner = "monochange"
repo = "monochange"

[source.releases]
branches = ["main"]
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	git_in_temp_repo(tempdir.path(), &["init", "-b", "main"]);
	git_in_temp_repo(
		tempdir.path(),
		&["config", "user.email", "test@example.com"],
	);
	git_in_temp_repo(tempdir.path(), &["config", "user.name", "Test User"]);
	git_in_temp_repo(tempdir.path(), &["config", "commit.gpgsign", "false"]);
	fs::write(tempdir.path().join("README.md"), "release branch\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));
	git_in_temp_repo(tempdir.path(), &["add", "."]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "initial"]);
	git_in_temp_repo(tempdir.path(), &["tag", "v1.0.0"]);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("verify-release-branch"),
		],
	)
	.unwrap_or_else(|error| panic!("verify release branch: {error}"));

	assert!(output.contains("release branch verified"));
	assert!(output.contains("main"));

	let tag_output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("verify-release-branch"),
			OsString::from("--from"),
			OsString::from("v1.0.0"),
		],
	)
	.unwrap_or_else(|error| panic!("verify release branch tag: {error}"));
	assert!(tag_output.contains("v1.0.0"));
}

#[tokio::test(flavor = "multi_thread")]
async fn publish_subcommands_dispatch_to_their_built_in_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("next-version-preview", tempdir.path());

	// `publish readiness` requires a release ref, so the failure it reports
	// proves the command routed to the `PublishReadiness` step rather than
	// stopping at argument parsing.
	let error = run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("publish"),
			OsString::from("readiness"),
		],
		tempdir.path(),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected publish readiness to require a ref"));

	assert_eq!(
		error.to_string(),
		"config error: missing required option `--from <ref>`; pass the release commit or tag to check, for example `--from HEAD`"
	);
}

#[test]
fn unknown_clap_help_path_includes_requested_path() {
	let command = super::build_command("monochange");
	let path = [String::from("not-a-command")];
	let output = super::render_unknown_clap_help_path("monochange", &command, &path);
	let root_output = super::render_unknown_clap_help_path("monochange", &command, &[]);
	let root_path =
		super::clap_help_path_from_args(&[OsString::from("monochange"), OsString::from("--help")]);
	let help_path = super::clap_help_path_from_args(&[
		OsString::from("monochange"),
		OsString::from("help"),
		OsString::from("--help"),
	]);

	assert!(output.contains("unrecognized command path `monochange not-a-command`"));
	assert!(output.contains("Visible top-level commands:"));
	assert!(root_output.contains("unrecognized command path `monochange`"));
	assert!(root_path.is_empty());
	assert_eq!(help_path, [String::from("help")]);
}

#[test]
fn subagents_command_supports_no_mcp_dry_run_json_output() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let output = run_cli(
		fixture.path(),
		[
			OsString::from("monochange"),
			OsString::from("subagents"),
			OsString::from("vscode"),
			OsString::from("copilot"),
			OsString::from("--no-mcp"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("subagents no-mcp dry-run json: {error}"));
	let value: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("parse json: {error}"));

	assert_readable_json_snapshot!("subagents_no_mcp_dry_run", value);
}

#[test]
fn subagents_command_requires_force_to_overwrite_existing_files() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let root = fixture.path();

	run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("subagents"),
			OsString::from("claude"),
		],
	)
	.unwrap_or_else(|error| panic!("write initial subagents: {error}"));
	fs::write(root.join(".mcp.json"), "custom\n")
		.unwrap_or_else(|error| panic!("overwrite test mcp config: {error}"));

	let error = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("subagents"),
			OsString::from("claude"),
		],
	)
	.expect_err("subagents should refuse to overwrite without --force");
	insta::assert_snapshot!("subagents_overwrite_error", error.to_string());

	run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("subagents"),
			OsString::from("claude"),
			OsString::from("--force"),
		],
	)
	.unwrap_or_else(|error| panic!("force overwrite subagents: {error}"));

	insta::assert_snapshot!(
		"subagents_forced_claude_mcp",
		fs::read_to_string(root.join(".mcp.json"))
			.unwrap_or_else(|error| panic!("read overwritten mcp: {error}"))
	);
}

#[test]
fn custom_command_unexpected_argument_error_explains_configured_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("monochange.toml"),
		r#"
[cli.publish]
help_text = "Publish packages"
inputs = [
	{ name = "package", type = "string_list" },
	{ name = "format", type = "choice", choices = ["text", "json"], default = "text" },
]
steps = [{ name = "noop", type = "Command", command = "true" }]
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));

	let error = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("run"),
			OsString::from("publish"),
			OsString::from("--all"),
		],
	)
	.expect_err("unexpected custom command input should fail");
	let report = error.render();

	assert!(report.contains("✖ Unexpected command input"));
	assert!(
		report.contains("Argument --all is not declared for custom command monochange run publish")
	);
	assert!(report.contains("monochange run publish"));
	assert!(report.contains("[--dry-run]"));
	assert!(report.contains("[--package <PACKAGE>]"));
	assert!(report.contains("[--format <FORMAT>]"));
	assert!(report.contains("This command comes from [cli.publish] in monochange.toml"));
	assert!(report.contains("{ name = \"all\", type = \"boolean\" }"));

	assert!(!report.contains("config error: error:"));
	assert!(!report.contains("[OPTIONS]"));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_record_jq_filters_json_output_for_ci() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut record = sample_release_record_for_retarget();
	record.release_targets[0].id = "main".to_string();
	create_release_record_commit_from_record(root, &record);
	let subscriber = tracing_subscriber::fmt()
		.with_max_level(tracing::Level::TRACE)
		.with_writer(std::io::sink)
		.finish();
	let traced_discovery = tracing::subscriber::with_default(subscriber, || {
		crate::release_record::discover_release_record(root, "HEAD")
	})
	.await
	.unwrap_or_else(|error| panic!("trace release-record discovery: {error}"));
	assert_eq!(traced_discovery.record.release_targets[0].id, "main");

	let tag = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
			OsString::from("--jq"),
			OsString::from(
				r#".record.release_targets[] | select(.id == "main" and .kind == "group" and .release == true) | .tag_name"#,
			),
		],
	)
	.unwrap_or_else(|error| panic!("release-record jq tag output: {error}"));
	assert_eq!(tag, "v1.2.3");

	let is_release_commit = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
			OsString::from("--jq"),
			OsString::from(".resolved_commit == .record_commit"),
		],
	)
	.unwrap_or_else(|error| panic!("release-record jq commit output: {error}"));
	assert_eq!(is_release_commit, "true");
}

#[test]
fn publish_readiness_dispatches_from_release_record_and_writes_artifact() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	create_release_record_commit(root);
	let output_path = root.join(".monochange/local/readiness.json");
	let output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("publish-readiness"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--package"),
			OsString::from("missing"),
			OsString::from("--format"),
			OsString::from("text"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("publish-readiness output: {error}"));

	assert!(output.contains("✔ Ready to publish"), "{output}");
	assert!(output.starts_with("Ref            HEAD\n"), "{output}");
	assert!(
		output.contains("No packages are selected for publishing."),
		"{output}"
	);

	let artifact = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read readiness artifact: {error}"));
	assert!(artifact.contains("\"status\": \"ready\""));
	assert!(artifact.contains("\"from\": \"HEAD\""));
	assert!(artifact.contains("\"packages\": []"));
}

#[test]
fn plan_release_uses_semantic_guardrail_evidence_for_changeset_covered_breaking_public_api() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = "semantic-release-guardrails/changeset-covered-breaking-public-api";
	copy_fixture(&format!("{fixture_root}/before"), tempdir.path());
	init_git_repo(tempdir.path());
	git_in_temp_repo(tempdir.path(), &["add", "."]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "initial fixture"]);
	copy_fixture(&format!("{fixture_root}/after"), tempdir.path());

	let plan = plan_release(
		tempdir.path(),
		&tempdir.path().join(".changeset/core-breaking-api.md"),
	)
	.unwrap_or_else(|error| panic!("release plan: {error}"));
	let core_decision = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("core"))
		.unwrap_or_else(|| panic!("expected core release decision"));
	let semantic_evidence = plan
		.compatibility_evidence
		.iter()
		.find(|assessment| assessment.provider_id.starts_with("semantic-analysis:"))
		.unwrap_or_else(|| panic!("expected semantic compatibility evidence"));

	assert_eq!(core_decision.recommended_bump, BumpSeverity::Major);
	assert_eq!(semantic_evidence.package_id, core_decision.package_id);
	assert_eq!(semantic_evidence.severity, BumpSeverity::Major);
	assert_eq!(semantic_evidence.confidence, "high");
	assert!(
		plan.warnings.is_empty(),
		"unexpected warnings: {:?}",
		plan.warnings
	);
}

#[test]
fn plan_release_aggregates_transitive_dependents_and_version_groups() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let plan = plan_release(&fixture_root, &fixture_root.join("changes-minor.md"))
		.unwrap_or_else(|error| panic!("release plan: {error}"));

	let sdk_core = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("cargo/sdk-core/Cargo.toml"))
		.unwrap_or_else(|| panic!("expected cargo core decision"));
	let web_sdk = plan
		.decisions
		.iter()
		.find(|decision| {
			decision
				.package_id
				.contains("packages/web-sdk/package.json")
		})
		.unwrap_or_else(|| panic!("expected web sdk decision"));
	let deno_tool = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("deno/tool/deno.json"))
		.unwrap_or_else(|| panic!("expected deno tool decision"));
	let mobile_sdk = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("dart/mobile_sdk/pubspec.yaml"))
		.unwrap_or_else(|| panic!("expected mobile sdk decision"));
	let version_group = plan
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected version group plan"));

	assert_eq!(sdk_core.recommended_bump.to_string(), "minor");
	assert_eq!(sdk_core.trigger_type, "direct-change");
	assert_eq!(web_sdk.recommended_bump.to_string(), "minor");
	assert_eq!(web_sdk.trigger_type, "version-group-synchronization");
	assert_eq!(deno_tool.recommended_bump.to_string(), "patch");
	assert_eq!(mobile_sdk.recommended_bump.to_string(), "patch");
	assert_eq!(
		version_group
			.planned_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("1.1.0")
	);
}

#[test]
fn changes_add_canonicalizes_package_references_to_package_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/canonicalize-package-ref", tempdir.path());
	let output_path = tempdir.path().join("repo-change.md");
	run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("create-change-file"),
			OsString::from("--package"),
			OsString::from("crates/monochange"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("canonical package names"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("monochange: patch"));
	assert!(!content.contains("crates/monochange: patch"));
}

#[test]
fn changes_add_defaults_bump_to_none_when_type_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture(
		"changeset-target-metadata/cli-type-only-change",
		tempdir.path(),
	);
	let output_path = tempdir.path().join("docs.md");

	run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("create-change-file"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("docs"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core: docs"));
	assert!(!content.contains("bump:"));
}

#[test]
fn render_change_target_markdown_renders_version_and_caused_by_context() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"core",
		BumpSeverity::Patch,
		Some("2.0.0"),
		None,
		&["sdk".to_string()],
	)
	.unwrap_or_else(|error| panic!("render version target: {error}"));
	assert_eq!(
		lines,
		vec![
			"core:".to_string(),
			"  bump: patch".to_string(),
			"  version: \"2.0.0\"".to_string(),
			"  caused_by: [\"sdk\"]".to_string(),
		]
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn command_versions_reports_planned_versions_without_mutating_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let workspace_manifest = tempdir.path().join("Cargo.toml");
	let core_changelog = tempdir.path().join("crates/core/changelog.md");
	let original_manifest = fs::read_to_string(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let original_changelog = fs::read_to_string(&core_changelog)
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
	let matches = crate::build_command_with_cli("monochange", &configuration.cli)
		.try_get_matches_from(["monochange", "step", "display-versions", "--format", "text"])
		.unwrap_or_else(|error| panic!("versions matches: {error}"));

	let (_, step_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected step namespace matches"));
	let versions_matches = step_matches
		.subcommand_matches("display-versions")
		.unwrap_or_else(|| panic!("expected versions subcommand matches"));
	let output = execute_matches(
		tempdir.path(),
		&configuration,
		"step display-versions",
		versions_matches,
		false,
	)
	.await
	.unwrap_or_else(|error| panic!("versions output: {error}"));

	assert!(
		output
			.lines()
			.last()
			.unwrap_or_default()
			.starts_with("✔ Next versions"),
		"{output}"
	);
	assert!(output.starts_with("Groups\n"), "{output}");
	assert_eq!(
		fs::read_to_string(&workspace_manifest)
			.unwrap_or_else(|error| panic!("workspace manifest after versions: {error}")),
		original_manifest
	);
	assert_eq!(
		fs::read_to_string(&core_changelog)
			.unwrap_or_else(|error| panic!("core changelog after versions: {error}")),
		original_changelog
	);
	assert!(tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_config_defaults_to_a_human_summary() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("config"),
		],
	)
	.unwrap_or_else(|error| panic!("config output: {error}"));

	assert!(output.starts_with("Workspace configuration\n\n"));
	assert!(output.contains("Path: monochange.toml"));
	assert!(output.contains("Packages: 2 (2 cargo)"));
	assert!(output.contains("Use `--format json`"));
	assert!(!output.trim_start().starts_with('{'));
}

#[test]
fn render_config_step_falls_back_to_uncanonicalized_root_for_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("config: {error}"));
	let missing_root = tempdir.path().join("missing-root");

	let output = crate::render_config_step(&missing_root, &configuration, OutputFormat::Json)
		.unwrap_or_else(|error| panic!("config json: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("config json: {error}"));

	assert_eq!(
		parsed["project_root"],
		serde_json::json!(missing_root.display().to_string())
	);
	assert_eq!(
		parsed["config_path"],
		serde_json::json!(missing_root.join("monochange.toml").display().to_string())
	);
}

#[test]
fn command_release_updates_inferred_cargo_lockfiles_without_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_cargo_lock_release_fixture(tempdir.path());

	with_path_prefixed(tempdir.path(), || {
		run_cli(
			tempdir.path(),
			[
				OsString::from("monochange"),
				OsString::from("step"),
				OsString::from("prepare-release"),
			],
		)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	});
	let cargo_lock = fs::read_to_string(tempdir.path().join("Cargo.lock"))
		.unwrap_or_else(|error| panic!("cargo lock: {error}"));

	assert!(cargo_lock.contains("version = \"1.1.0\""));
}

#[test]
fn command_release_updates_inferred_bun_lockfiles_without_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_bun_lockb_release_fixture(tempdir.path());

	with_path_prefixed(tempdir.path(), || {
		run_cli(
			tempdir.path(),
			[
				OsString::from("monochange"),
				OsString::from("step"),
				OsString::from("prepare-release"),
			],
		)
		.unwrap_or_else(|error| panic!("command output: {error}"));
	});
	let bun_lock = fs::read(tempdir.path().join("packages/app/bun.lockb"))
		.unwrap_or_else(|error| panic!("bun lockb: {error}"));

	assert!(String::from_utf8_lossy(&bun_lock).contains("1.1.0"));
}

#[test]
fn command_release_honors_explicit_lockfile_paths_in_versioned_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_explicit_lockfile_override_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("prepare-release"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let shared_lock = fs::read_to_string(tempdir.path().join("lockfiles/shared/package-lock.json"))
		.unwrap_or_else(|error| panic!("shared package lock: {error}"));

	assert!(shared_lock.contains("\"version\": \"1.1.0\""));
}

#[test]
fn command_release_updates_regex_versioned_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/regex-versioned-file", tempdir.path());

	run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("prepare-release"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let readme = fs::read_to_string(tempdir.path().join("README.md"))
		.unwrap_or_else(|error| panic!("readme: {error}"));

	assert!(readme.contains("https://example.com/download/v1.1.0.tgz"));
	assert!(!readme.contains("https://example.com/download/v1.0.0.tgz"));
}

#[test]
fn command_diagnostics_supports_absolute_changeset_path() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);
	let absolute = tempdir.path().join(".changeset/feature.md");

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("diagnose-changesets"),
			OsString::from("--format"),
			OsString::from("text"),
			OsString::from("--changeset"),
			OsString::from(absolute.to_string_lossy().into_owned()),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.starts_with(".changeset/feature.md\n"), "{output}");
}

#[test]
fn planning_behavior_is_consistent_across_ecosystem_fixtures() {
	assert_simple_release_pattern(
		"../../fixtures/cargo/workspace",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_simple_release_pattern(
		"../../fixtures/npm/workspace",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/deno/workspace",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/dart/workspace",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

#[test]
fn command_release_dry_run_is_consistent_across_ecosystem_fixtures() {
	assert_cli_release_pattern(
		"../../fixtures/cargo/workspace",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_cli_release_pattern(
		"../../fixtures/npm/workspace",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/deno/workspace",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/dart/workspace",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

fn assert_simple_release_pattern(
	relative_fixture_root: &str,
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());
	let changes_path = tempdir.path().join(".changeset/feature.md");

	let discovery = discover_workspace(tempdir.path())
		.unwrap_or_else(|error| panic!("fixture discovery: {error}"));
	assert_eq!(discovery.packages.len(), 2);
	assert_eq!(discovery.dependencies.len(), 1);

	let plan = plan_release(tempdir.path(), &changes_path)
		.unwrap_or_else(|error| panic!("fixture release plan: {error}"));
	let direct = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains(direct_manifest_suffix))
		.unwrap_or_else(|| panic!("expected direct decision"));
	let dependent = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains(dependent_manifest_suffix))
		.unwrap_or_else(|| panic!("expected dependent decision"));

	assert_eq!(direct.recommended_bump.to_string(), "minor");
	assert_eq!(dependent.recommended_bump.to_string(), "patch");
}

fn assert_cli_release_pattern(
	relative_fixture_root: &str,
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("prepare-release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("json: {error}"));
	let decisions = parsed["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("plan decisions"));
	let direct = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(direct_manifest_suffix))
		})
		.unwrap_or_else(|| panic!("expected direct release decision"));
	let dependent = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(dependent_manifest_suffix))
		})
		.unwrap_or_else(|| panic!("expected dependent release decision"));

	assert_eq!(direct["bump"].as_str(), Some("minor"));
	assert_eq!(dependent["bump"].as_str(), Some("patch"));
}

fn seed_diagnostics_fixture(root: &Path, with_second_changeset: bool) {
	if with_second_changeset {
		copy_fixture("monochange/diagnostics-two-changesets", root);
	} else {
		copy_fixture("monochange/diagnostics-base", root);
	}
}

fn seed_release_fixture(root: &Path, command_step: Option<&str>, failing_changelog: bool) {
	if failing_changelog {
		copy_fixture("monochange/release-failing-changelog", root);
	} else if command_step.is_some() {
		copy_fixture("monochange/release-with-command-step", root);
	} else {
		copy_fixture("monochange/release-base", root);
	}

	let _ = command_step;
}

fn seed_cargo_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/cargo-lock-release", root);
}

fn seed_bun_lockb_release_fixture(root: &Path) {
	copy_fixture("monochange/bun-lock-release", root);
}

fn seed_explicit_lockfile_override_fixture(root: &Path) {
	copy_fixture("monochange/explicit-lockfile-override", root);
}

#[test]
fn validate_rejects_workspace_versioned_packages_in_different_groups() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-different-groups");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("validate"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	let rendered = error.to_string();

	assert!(rendered.contains("version.workspace = true"));
	assert!(rendered.contains("same version group"));
}

#[test]
fn command_step_with_id_captures_stdout_for_later_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("monochange"), OsString::from("echo-test")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("got:hello-world"),
		"expected stdout interpolation, got: {output}"
	);
}

fn seed_step_outputs_fixture(root: &Path) {
	let fixture_dir =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/step-outputs/base");
	let files = [
		"Cargo.toml",
		"monochange.toml",
		"crates/core/Cargo.toml",
		"crates/core/src/lib.rs",
		"crates/core/CHANGELOG.md",
		".changeset/feature.md",
	];

	for file in &files {
		let source = fixture_dir.join(file);
		let target = root.join(file);

		if let Some(parent) = target.parent() {
			fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
		}

		fs::copy(&source, &target)
			.unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
	}
}

#[cfg(unix)]
fn make_executable(path: &Path) {
	let metadata =
		fs::metadata(path).unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()));
	let mut permissions = metadata.permissions();
	permissions.set_mode(0o755);
	fs::set_permissions(path, permissions)
		.unwrap_or_else(|error| panic!("set permissions {}: {error}", path.display()));
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn with_path_prefixed<T>(root: &Path, action: impl FnOnce() -> T) -> T {
	let bin_dir = root.join("tools/bin");

	for tool in ["cargo", "npm", "npx", "pnpm", "bun", "bunx", "custom-lock"] {
		let candidate = bin_dir.join(tool);

		if candidate.exists() {
			make_executable(&candidate);
		}
	}

	let existing = std::env::var_os("PATH").unwrap_or_default();
	let mut combined = std::env::split_paths(&existing).collect::<Vec<_>>();
	combined.insert(0, bin_dir);
	let new_path =
		std::env::join_paths(combined).unwrap_or_else(|error| panic!("join PATH entries: {error}"));
	temp_env::with_var("PATH", Some(new_path), action)
}

fn copy_fixture(fixture_relative: &str, dest: &Path) {
	copy_directory(&fixture_path(fixture_relative), dest);
}

pub(crate) fn cli_context_for_tests() -> CliContext {
	CliContext {
		root: PathBuf::from("."),
		dry_run: false,
		quiet: false,
		show_diff: false,
		output_format: OutputFormat::Text,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		originating_command: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
		command_results: Vec::new(),
	}
}

#[test]
fn should_execute_cli_step_gates_on_release_streams_and_outputs() {
	let mut context = cli_context_for_tests();
	let changelog = |output: &str, stream: &str| {
		crate::PreparedChangelog {
			owner_id: "main".to_string(),
			owner_kind: monochange_core::ReleaseOwnerKind::Group,
			output: output.to_string(),
			stream: stream.to_string(),
			path: PathBuf::from("deploy/upgrade-notes/2.0.0.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: monochange_core::ReleaseNotesDocument {
				title: String::new(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: String::new(),
		}
	};
	context.prepared_release = Some(crate::PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["transfer".to_string()],
		package_publications: Vec::new(),
		version: Some("2.0.0".to_string()),
		group_version: Some("2.0.0".to_string()),
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		// A duplicated default stream exercises the deduplicated stream list.
		changelogs: vec![
			changelog("upgrade_notes", "onchain"),
			changelog("default", "default"),
			changelog("default", "default"),
		],
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: false,
		versioning: crate::versioning_state::ResolvedReleaseValues::default(),
	});
	let step_inputs = BTreeMap::new();
	let step_for = |condition: &str| {
		monochange_core::CliStepDefinition::Command {
			show_progress: None,
			name: None,
			when: Some(condition.to_string()),
			always_run: false,
			command: "printf hi".to_string(),
			dry_run_command: None,
			shell: monochange_core::ShellConfig::default(),
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		}
	};
	let evaluate = |condition: &str| {
		should_execute_cli_step(&step_for(condition), &context, &step_inputs)
			.unwrap_or_else(|error| panic!("when condition `{condition}`: {error}"))
	};
	assert!(
		evaluate("{{ 'onchain' in release.streams }}"),
		"a release that rendered the onchain stream owes the deploy step"
	);
	assert!(
		!evaluate("{{ 'store' in release.streams }}"),
		"a stream the release did not render must not gate the step on"
	);

	assert!(
		evaluate("{{ 'upgrade_notes' in release.outputs }}"),
		"named outputs are exposed alongside streams"
	);
}

#[test]
fn should_execute_cli_step_rejects_non_scalar_condition_value() {
	let context = cli_context_for_tests();
	let step_inputs =
		BTreeMap::from([("list".to_string(), vec!["a".to_string(), "b".to_string()])]);
	let step = monochange_core::CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: Some("{{ inputs.list }}".to_string()),
		always_run: false,
		command: "printf hi".to_string(),
		dry_run_command: None,
		shell: monochange_core::ShellConfig::default(),
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	let error = should_execute_cli_step(&step, &context, &step_inputs).unwrap_err();
	assert!(error.to_string().contains("is not a scalar boolean value"));
}

#[test]
fn template_value_to_input_values_array_flattens_elements() {
	use serde_json::json;

	use super::template_value_to_input_values;
	assert_eq!(
		template_value_to_input_values(&json!(["a", "b", "c"])),
		vec!["a", "b", "c"]
	);
	assert_eq!(
		template_value_to_input_values(&json!([true, false])),
		vec!["true", "false"]
	);
}

fn sample_release_manifest_for_commit_message(
	include_changelog: bool,
	include_deleted_changeset: bool,
) -> monochange_core::ReleaseManifest {
	monochange_core::ReleaseManifest {
		command: "release-pr".to_string(),
		dry_run: false,
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string(), "monochange_core".to_string()],
			rendered_title: "monochange 1.2.3".to_string(),
			rendered_changelog_title: "1.2.3".to_string(),
			floating_tags: Vec::new(),
		}],
		released_packages: vec!["monochange".to_string(), "monochange_core".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],

		changelogs: if include_changelog {
			vec![monochange_core::ReleaseManifestChangelog {
				owner_id: "sdk".to_string(),
				owner_kind: monochange_core::ReleaseOwnerKind::Group,
				output: "default".to_string(),
				stream: "default".to_string(),
				path: Path::new("crates/monochange/CHANGELOG.md").to_path_buf(),
				format: monochange_core::ChangelogFormat::Monochange,
				notes: monochange_core::ReleaseNotesDocument {
					title: "1.2.3".to_string(),
					summary: vec!["- prepare release".to_string()],
					sections: Vec::new(),
				},
				rendered: "## 1.2.3".to_string(),
			}]
		} else {
			Vec::new()
		},

		changesets: Vec::new(),

		deleted_changesets: if include_deleted_changeset {
			vec![Path::new(".changeset/feature.md").to_path_buf()]
		} else {
			Vec::new()
		},

		package_publications: Vec::new(),
		plan: monochange_core::ReleaseManifestPlan {
			workspace_root: Path::new(".").to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	}
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn commit_release_command_creates_local_commit_with_release_record() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("prepared-release/source-github-follow-up/workspace", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	// The fixture defines [cli.commit-release] with PrepareRelease and
	// CommitRelease steps, so running commit-release will prepare the
	// release and then commit it.
	let output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("commit-release"),
		],
	)
	.unwrap_or_else(|error| panic!("commit-release output: {error}"));
	let commit_subject = git_output_in_temp_repo(root, &["log", "-1", "--pretty=%s"]);
	let _commit_body = git_output_in_temp_repo(root, &["log", "-1", "--pretty=%B"]);
	let status = git_output_in_temp_repo(root, &["status", "--short"]);

	assert!(output.contains("\nRelease commit\n"), "{output}");
	assert!(output.contains("tracked path"), "{output}");
	assert_eq!(commit_subject, "chore(release): prepare release");
	assert!(
		status.is_empty(),
		"expected clean working tree, got: {status}"
	);
	let release_files: Vec<_> = fs::read_dir(root.join(".monochange/releases"))
		.unwrap_or_else(|error| panic!("read releases dir: {error}"))
		.filter_map(Result::ok)
		.filter(|entry| entry.path().join("release.json").exists())
		.collect();
	assert!(
		!release_files.is_empty(),
		"expected at least one release record file in .monochange/releases/"
	);
}

#[test]
fn commit_release_command_reports_markdown_output_when_requested() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("prepared-release/source-github-follow-up/workspace", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	// Run commit-release in dry-run mode; the fixture has PrepareRelease +
	// CommitRelease steps, so the command name in the dry-run header is
	// `commit-release`.
	let output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("commit-release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("markdown"),
		],
	)
	.unwrap_or_else(|error| panic!("commit-release output: {error}"));

	assert!(output.contains("# `commit-release` (dry-run)"));
}

#[allow(clippy::large_futures)]
#[tokio::test(flavor = "multi_thread")]
async fn repair_release_command_emits_retarget_progress_statuses() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);

	let output = temp_env::async_with_vars([("MONOCHANGE_PROGRESS_FORMAT", Some("json"))], async {
		run_with_args_in_dir(
			"monochange",
			[
				OsString::from("monochange"),
				OsString::from("step"),
				OsString::from("retarget-release"),
				OsString::from("--from"),
				OsString::from("v1.2.3"),
				OsString::from("--target"),
				OsString::from("HEAD"),
				OsString::from("--sync-provider=false"),
				OsString::from("--dry-run"),
			],
			root,
		)
		.await
	})
	.await
	.unwrap_or_else(|error| panic!("repair-release output: {error}"));

	assert!(output.contains("• Would move"), "{output}");
	assert!(output.contains("dry-run"), "{output}");
}

#[test]
fn template_context_exposes_publish_namespace() {
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		output_format: OutputFormat::Text,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		originating_command: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: Some(crate::package_publish::PackagePublishReport {
			mode: crate::package_publish::PackagePublishRunMode::Release,
			dry_run: true,
			packages: vec![crate::package_publish::PackagePublishOutcome {
				package: "@monochange/skill".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: "npm".to_string(),
				version: "1.2.3".to_string(),
				status: crate::package_publish::PackagePublishStatus::Planned,
				message: "would publish package".to_string(),
				placeholder: false,
				trusted_publishing: crate::package_publish::TrustedPublishingOutcome {
					status: crate::package_publish::TrustedPublishingStatus::Planned,
					repository: Some("ifiokjr/monochange".to_string()),
					workflow: Some("publish.yml".to_string()),
					environment: None,
					setup_url: Some(
						"https://docs.npmjs.com/cli/v11/commands/npm-trust".to_string(),
					),
					message: "would configure trusted publishing".to_string(),
				},
				command: None,
				stdout: None,
				stderr: None,
				rate_limit: None,
				deferred_to: None,
			}],
		}),
		rate_limit_report: Some(monochange_core::PublishRateLimitReport {
			dry_run: true,
			windows: vec![monochange_core::RegistryRateLimitWindowPlan {
				registry: monochange_core::RegistryKind::Npm,
				operation: monochange_core::RateLimitOperation::Publish,
				limit: None,
				window_seconds: None,
				pending: 1,
				batches_required: 1,
				fits_single_window: true,
				confidence: monochange_core::RateLimitConfidence::Low,
				notes: "npm soft limit".to_string(),
				evidence: Vec::new(),
			}],
			batches: vec![monochange_core::PublishRateLimitBatch {
				registry: monochange_core::RegistryKind::Npm,
				operation: monochange_core::RateLimitOperation::Publish,
				batch_index: 1,
				total_batches: 1,
				packages: vec!["@monochange/skill".to_string()],
				recommended_wait_seconds: None,
			}],
			warnings: Vec::new(),
		}),
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
		command_results: Vec::new(),
	};
	let template_context = build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/mode"))
			.and_then(serde_json::Value::as_str),
		Some("release")
	);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/packages/0/package"))
			.and_then(serde_json::Value::as_str),
		Some("@monochange/skill")
	);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/packages/0/trusted_publishing/status"))
			.and_then(serde_json::Value::as_str),
		Some("planned")
	);
	assert_eq!(
		template_context
			.get("publish")
			.and_then(|value| value.pointer("/rate_limits/batches/0/packages/0"))
			.and_then(serde_json::Value::as_str),
		Some("@monochange/skill")
	);
}

#[test]
fn template_context_exposes_manifest_affected_steps_and_custom_variables() {
	let mut step_outputs = BTreeMap::new();
	step_outputs.insert(
		"lint".to_string(),
		crate::CommandStepOutput {
			stdout: "ok".to_string(),
			stderr: String::new(),
		},
	);
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		quiet: false,
		show_diff: false,
		output_format: OutputFormat::Text,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		originating_command: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: Some(monochange_core::ChangesetPolicyEvaluation {
			status: monochange_core::ChangesetPolicyStatus::Passed,
			required: true,
			enforce: false,
			summary: "covered".to_string(),
			comment: None,
			labels: Vec::new(),
			matched_skip_labels: Vec::new(),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			matched_paths: vec!["crates/core/src/lib.rs".to_string()],
			ignored_paths: Vec::new(),
			changeset_paths: vec![".changeset/core.md".to_string()],
			affected_package_ids: vec!["core".to_string()],
			covered_package_ids: vec!["core".to_string()],
			uncovered_package_ids: Vec::new(),
			warnings: Vec::new(),
			errors: Vec::new(),
		}),
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs,
		command_logs: Vec::new(),
		command_results: Vec::new(),
	};
	let inputs = BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
	let variables = BTreeMap::from([
		(
			"custom_version".to_string(),
			monochange_core::CommandVariable::Version,
		),
		(
			"custom_changesets".to_string(),
			monochange_core::CommandVariable::Changesets,
		),
	]);
	let template_context = build_cli_template_context(&context, &inputs, Some(&variables));
	assert_eq!(
		template_context
			.get("manifest")
			.and_then(|value| value.pointer("/path"))
			.and_then(serde_json::Value::as_str),
		Some("target/release-manifest.json")
	);
	assert_eq!(
		template_context
			.get("affected")
			.and_then(|value| value.pointer("/status"))
			.and_then(serde_json::Value::as_str),
		Some("passed")
	);

	assert_eq!(
		template_context
			.get("steps")
			.and_then(|value| value.pointer("/lint/stdout"))
			.and_then(serde_json::Value::as_str),
		Some("ok")
	);
	assert_eq!(
		template_context
			.get("custom_version")
			.and_then(serde_json::Value::as_str),
		Some("1.2.3")
	);
	assert_eq!(
		template_context
			.get("custom_changesets")
			.and_then(serde_json::Value::as_str),
		Some("")
	);
	assert_eq!(
		template_context
			.get("inputs")
			.and_then(|value| value.pointer("/format"))
			.and_then(serde_json::Value::as_str),
		Some("json")
	);
	assert!(
		template_context.get("format").is_none(),
		"CLI inputs should only be available under the inputs namespace"
	);
	assert_eq!(
		template_context
			.get("released_packages_list")
			.and_then(serde_json::Value::as_array)
			.map(Vec::len),
		Some(1)
	);
	assert_eq!(
		template_context
			.get("number_of_changesets")
			.and_then(serde_json::Value::as_u64),
		Some(0)
	);
	assert_eq!(
		template_context
			.get("changeset_count")
			.and_then(serde_json::Value::as_u64),
		Some(0)
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_source_follow_up_steps_require_source_configuration() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	let root = tempdir.path();
	let mut configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.source = Some(monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: Some("https://gitlab.example.com".to_string()),
		api_url: Some("https://gitlab.example.com/api/v4".to_string()),
		owner: "org".to_string(),
		repo: "repo".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	});
	let prepare_and_publish = CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				always_run: false,
				inputs: BTreeMap::new(),
			},
		],
		dry_run: false,
	};
	let error = execute_cli_command(
		root,
		&configuration,
		&prepare_and_publish,
		true,
		BTreeMap::new(),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected github source requirement error"));

	assert!(
		error.to_string().contains(
			"`CommentReleasedIssues` is not supported for `[source].provider = \"gitlab\"`"
		)
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_prepare_release_writes_default_manifest_cache_and_follow_up_steps_render_dry_run_outputs()
 {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	fs::OpenOptions::new()
		.append(true)
		.open(tempdir.path().join("monochange.toml"))
		.and_then(|mut file| {
			use std::io::Write;
			writeln!(
				file,
				"\n[source]\nprovider = \"github\"\nowner = \"ifiokjr\"\nrepo = \"monochange\"\n"
			)
		})
		.unwrap_or_else(|error| panic!("append source config: {error}"));
	let root = tempdir.path();
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));

	let manifest_path = root.join(".monochange/local/release-manifest.json");
	let prepare_release = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::PrepareRelease {
			name: None,
			when: None,
			always_run: false,
			inputs: text_format_step_inputs(),
			allow_empty_changesets: false,
		}],
		dry_run: false,
	};
	let render_output = execute_cli_command(
		root,
		&configuration,
		&prepare_release,
		true,
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.await
	.unwrap_or_else(|error| panic!("prepare release: {error}"));
	assert!(render_output.contains("Manifest  .monochange/local/release-manifest.json"));
	let manifest_contents =
		fs::read_to_string(&manifest_path).unwrap_or_else(|error| panic!("read manifest: {error}"));
	assert!(manifest_contents.contains("\"release_targets\""));

	let publish_release = CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: text_format_step_inputs(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: text_format_step_inputs(),
			},
		],
		dry_run: false,
	};
	let publish_output = execute_cli_command(
		root,
		&configuration,
		&publish_release,
		true,
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.await
	.unwrap_or_else(|error| panic!("publish release: {error}"));
	assert!(
		publish_output.contains("Provider releases"),
		"{publish_output}"
	);
	assert!(publish_output.contains("dry-run"));

	let release_request = CliCommandDefinition {
		name: "release-pr".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: text_format_step_inputs(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				always_run: false,
				no_verify: false,
				stage_all: false,
				backend: monochange_core::ReleaseBackend::default(),
				hosted_auth: monochange_core::HostedCommitAuth::default(),
				hosted_url: None,
				inputs: text_format_step_inputs(),
			},
		],
		dry_run: false,
	};
	let request_output = execute_cli_command(
		root,
		&configuration,
		&release_request,
		true,
		BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
	)
	.await
	.unwrap_or_else(|error| panic!("open release request: {error}"));
	assert!(request_output.contains("\nRelease request\n"));
	assert!(request_output.contains("dry-run"));

	let issue_comments = CliCommandDefinition {
		name: "release-comments".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				always_run: false,
				inputs: text_format_step_inputs(),
				allow_empty_changesets: false,
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				always_run: false,
				inputs: text_format_step_inputs(),
			},
		],
		dry_run: false,
	};
	let comments_output =
		execute_cli_command(root, &configuration, &issue_comments, true, BTreeMap::new())
			.await
			.unwrap_or_else(|error| panic!("comment released issues: {error}"));
	assert!(!comments_output.is_empty());
}

/// Serve a missing package followed by a registry error, using only loopback.
fn spawn_failing_test_registry() -> (
	std::net::SocketAddr,
	std::sync::Arc<std::sync::atomic::AtomicBool>,
	std::thread::JoinHandle<()>,
) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind test registry: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("test registry address: {error}"));
	listener
		.set_nonblocking(true)
		.unwrap_or_else(|error| panic!("set test registry listener nonblocking: {error}"));
	let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
	let thread_finished = std::sync::Arc::clone(&finished);
	let thread = std::thread::spawn(move || {
		let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
		let mut served = 0;

		while served < 2 && std::time::Instant::now() < deadline {
			if thread_finished.load(std::sync::atomic::Ordering::Relaxed) {
				break;
			}

			let mut stream = match listener.accept() {
				Ok((stream, _)) => stream,
				Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
					std::thread::sleep(std::time::Duration::from_millis(10));
					continue;
				}

				Err(error) => panic!("accept test registry connection: {error}"),
			};

			// macOS accepts inherit O_NONBLOCK; headers can arrive after accept.
			stream
				.set_nonblocking(false)
				.unwrap_or_else(|error| panic!("set test registry connection blocking: {error}"));
			stream
				.set_read_timeout(Some(std::time::Duration::from_secs(5)))
				.unwrap_or_else(|error| panic!("set test registry read timeout: {error}"));
			stream
				.set_write_timeout(Some(std::time::Duration::from_secs(5)))
				.unwrap_or_else(|error| panic!("set test registry write timeout: {error}"));

			if !read_test_registry_headers(&mut stream) {
				continue;
			}

			let response: &[u8] = if served == 0 {
				b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
			} else {
				b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
			};

			std::io::Write::write_all(&mut stream, response)
				.unwrap_or_else(|error| panic!("write test registry response: {error}"));
			served += 1;
		}
	});

	(address, finished, thread)
}

/// Wait for complete HTTP headers; an unused connection does not count as a request.
fn read_test_registry_headers(stream: &mut std::net::TcpStream) -> bool {
	let mut request = Vec::new();
	let mut buffer = [0_u8; 2048];

	while !request.windows(4).any(|bytes| bytes == b"\r\n\r\n") {
		let read = std::io::Read::read(stream, &mut buffer)
			.unwrap_or_else(|error| panic!("read test registry request headers: {error}"));

		if read == 0 {
			assert!(
				request.is_empty(),
				"test registry request ended before its headers"
			);

			return false;
		}

		request.extend_from_slice(
			buffer
				.get(..read)
				.unwrap_or_else(|| panic!("invalid test registry read length {read}")),
		);
		assert!(
			request.len() <= 16 * 1024,
			"test registry request headers exceed 16 KiB"
		);
	}

	true
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_placeholder_publish_step_surfaces_publish_execution_failure() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	// Trusted publishing is disabled so the test exercises the artifact-write
	// failure path instead of the trusted-publishing preflight, which would
	// block the run before the registry mock is ever contacted on CI (where
	// GITHUB_* environment variables are set).
	fs::write(
		root.join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n\n[package.pkg.publish.trusted_publishing]\nenabled = false\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	fs::create_dir_all(root.join("packages/pkg")).unwrap_or_else(|error| panic!("mkdir: {error}"));
	fs::write(
		root.join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.unwrap_or_else(|error| panic!("write package.json: {error}"));
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));

	let placeholder_command = CliCommandDefinition {
		name: "placeholder-publish".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::PlaceholderPublish {
			name: None,
			when: None,
			always_run: false,
			inputs: inherited_format_package_step_inputs(),
		}],
		dry_run: false,
	};

	let (registry_address, flow_finished, registry_thread) = spawn_failing_test_registry();

	temp_env::async_with_vars(
		[(
			"MONOCHANGE_NPM_REGISTRY_URL",
			Some(format!("http://{registry_address}")),
		)],
		async {
			let error = execute_cli_command(
				root,
				&configuration,
				&placeholder_command,
				true,
				BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
			)
			.await
			.expect_err("registry failure during placeholder publish should fail");
			assert!(
				error.render().contains("npm registry lookup"),
				"unexpected error: {}",
				error.render()
			);
		},
	)
	.await;
	flow_finished.store(true, std::sync::atomic::Ordering::Relaxed);
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_publish_packages_step_surfaces_publish_execution_failure() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	fs::create_dir_all(root.join("packages/pkg")).unwrap_or_else(|error| panic!("mkdir: {error}"));
	fs::write(
		root.join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.unwrap_or_else(|error| panic!("write package.json: {error}"));
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));

	let mut publish_step_inputs = inherited_format_package_step_inputs();
	publish_step_inputs.insert(
		"all".to_string(),
		monochange_core::CliStepInputValue::Inherited,
	);

	let publish_command = CliCommandDefinition {
		name: "publish".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![CliStepDefinition::PublishPackages {
			name: None,
			when: None,
			always_run: false,
			inputs: publish_step_inputs,
		}],
		dry_run: false,
	};

	// The plan step's `filter_pending_publish_requests` lookup returns 404
	// (version not yet published), so planning succeeds. The subsequent publish
	// execution lookup returns 500, surfacing a report-carrying failure through
	// the CLI runtime's publish-packages error branch.
	let registry = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind test registry: {error}"));
	let registry_address = registry
		.local_addr()
		.unwrap_or_else(|error| panic!("registry address: {error}"));
	let registry_thread = std::thread::spawn(move || {
		let mut served_not_found = false;
		for _ in 0..2 {
			let Ok((mut stream, _)) = registry.accept() else {
				break;
			};
			let mut request = [0_u8; 2048];
			let _ = std::io::Read::read(&mut stream, &mut request);
			let response: &[u8] = if served_not_found {
				b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
			} else {
				served_not_found = true;
				b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
			};

			let _ = std::io::Write::write_all(&mut stream, response);
		}
	});

	temp_env::async_with_vars(
		[(
			"MONOCHANGE_NPM_REGISTRY_URL",
			Some(format!("http://{registry_address}")),
		)],
		async {
			let error = execute_cli_command(
				root,
				&configuration,
				&publish_command,
				true,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("all".to_string(), vec!["true".to_string()]),
				]),
			)
			.await
			.expect_err("registry failure during publish should fail");
			assert!(
				error.render().contains("npm registry lookup"),
				"unexpected error: {}",
				error.render()
			);
		},
	)
	.await;
	registry_thread
		.join()
		.unwrap_or_else(|_| panic!("test registry thread panicked"));
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_allows_package_publish_steps_without_readiness_or_matching_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	let root = tempdir.path();
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));
	let server = MockServer::start();
	let _registry = server.mock(|when, then| {
		when.method(GET);
		then.status(404);
	});
	temp_env::async_with_vars(
		[("MONOCHANGE_CRATES_IO_API_URL", Some(server.base_url()))],
		async {
			let placeholder_command = CliCommandDefinition {
				name: "placeholder-publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PlaceholderPublish {
					name: None,
					when: None,
					always_run: false,
					inputs: inherited_format_package_step_inputs(),
				}],
				dry_run: false,
			};
			let placeholder_output = execute_cli_command(
				root,
				&configuration,
				&placeholder_command,
				false,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["missing-package".to_string()]),
				]),
			)
			.await
			.unwrap_or_else(|error| panic!("non-dry-run placeholder publish: {error}"));
			// Progress shows the rate-limit plan before publishing, so the
			// result ends with what happened instead of repeating the plan.
			assert_eq!(
				placeholder_output,
				"• No packages matched placeholder publishing criteria"
			);

			let publish_command = CliCommandDefinition {
				name: "publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![
					monochange_core::CliStepDefinition::PrepareRelease {
						name: None,
						when: None,
						always_run: false,
						inputs: text_format_step_inputs(),
						allow_empty_changesets: false,
					},
					monochange_core::CliStepDefinition::PublishPackages {
						name: None,
						when: None,
						always_run: false,
						inputs: inherited_format_package_step_inputs(),
					},
				],
				dry_run: false,
			};
			let publish_output = execute_cli_command(
				root,
				&configuration,
				&publish_command,
				false,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["missing-package".to_string()]),
				]),
			)
			.await
			.unwrap_or_else(|error| {
				panic!("non-dry-run publish packages without readiness: {error}")
			});
			assert_eq!(
				publish_output.lines().last(),
				Some("• No packages matched publishing criteria")
			);

			let release_tempdir =
				tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
			let release_root = release_tempdir.path();
			create_release_record_commit(release_root);
			let release_configuration = load_workspace_configuration(release_root)
				.unwrap_or_else(|error| panic!("release configuration: {error}"));
			let publish_result_path = release_root.join("publish-result.json");
			let publish_release_command = CliCommandDefinition {
				name: "publish".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PublishPackages {
					name: None,
					when: None,
					always_run: false,
					inputs: inherited_format_package_output_step_inputs(),
				}],
				dry_run: false,
			};
			let publish_output = execute_cli_command(
				release_root,
				&release_configuration,
				&publish_release_command,
				false,
				BTreeMap::from([
					("format".to_string(), vec!["text".to_string()]),
					("package".to_string(), vec!["missing-package".to_string()]),
					(
						"output".to_string(),
						vec![publish_result_path.to_string_lossy().to_string()],
					),
				]),
			)
			.await
			.unwrap_or_else(|error| panic!("publish packages without readiness: {error}"));
			assert_eq!(
				publish_output.lines().last(),
				Some("• No packages matched publishing criteria")
			);
			let publish_result = fs::read_to_string(&publish_result_path)
				.unwrap_or_else(|error| panic!("read publish result: {error}"));
			assert!(publish_result.contains("\"mode\": \"release\""));

			let plan_command = CliCommandDefinition {
				name: "publish-plan".to_string(),
				help_text: None,
				inputs: Vec::new(),
				steps: vec![monochange_core::CliStepDefinition::PlanPublishRateLimits {
					name: None,
					when: None,
					always_run: false,
					inputs: inherited_publish_plan_step_inputs(),
				}],
				dry_run: false,
			};
			let plan_output = execute_cli_command(
				root,
				&configuration,
				&plan_command,
				false,
				BTreeMap::from([
					("package".to_string(), vec!["missing-package".to_string()]),
					("mode".to_string(), vec!["placeholder".to_string()]),
				]),
			)
			.await
			.unwrap_or_else(|error| panic!("publish plan without matches: {error}"));
			assert_eq!(
				plan_output,
				"• No publish operations matched the current plan"
			);
		},
	)
	.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn release_follow_up_helpers_render_real_operation_outputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest = sample_release_manifest_for_commit_message(true, true);
	let written_path = crate::cli_runtime::write_release_manifest_file(
		tempdir.path(),
		Path::new("target/release-manifest.json"),
		&manifest,
	)
	.await
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	assert_eq!(written_path, PathBuf::from("target/release-manifest.json"));
	let manifest_contents =
		fs::read_to_string(tempdir.path().join(&written_path)).unwrap_or_else(|error| {
			panic!("read written manifest {}: {error}", written_path.display())
		});
	assert!(manifest_contents.contains("\"release_targets\""));

	let release_requests = vec![monochange_core::SourceReleaseRequest {
		provider: monochange_core::SourceProvider::GitHub,
		repository: "ifiokjr/monochange".to_string(),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		target_id: "sdk".to_string(),
		target_kind: monochange_core::ReleaseOwnerKind::Group,
		tag_name: "v1.2.3".to_string(),
		name: "monochange 1.2.3".to_string(),
		body: Some("body".to_string()),
		draft: false,
		prerelease: false,
		generate_release_notes: true,
	}];
	let release_results =
		crate::cli_runtime::build_release_results(false, &release_requests, || {
			Ok(vec![monochange_core::SourceReleaseOutcome {
				provider: monochange_core::SourceProvider::GitHub,
				repository: "ifiokjr/monochange".to_string(),
				tag_name: "v1.2.3".to_string(),
				operation: monochange_core::SourceReleaseOperation::Created,
				url: Some("https://example.com/releases/1".to_string()),
			}])
		})
		.unwrap_or_else(|error| panic!("render release results: {error}"));
	assert_eq!(
		release_results,
		vec!["ifiokjr/monochange v1.2.3 (created) via github".to_string()]
	);

	let release_request = monochange_core::SourceChangeRequest {
		provider: monochange_core::SourceProvider::GitHub,
		repository: "ifiokjr/monochange".to_string(),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		base_branch: "main".to_string(),
		head_branch: "release/v1.2.3".to_string(),
		title: "chore(release): prepare release".to_string(),
		body: "body".to_string(),
		labels: vec!["release".to_string()],
		auto_merge: true,
		commit_message: monochange_core::CommitMessage {
			subject: "subject".to_string(),
			body: Some("body".to_string()),
		},
		body_truncation: None,
	};
	let release_request_result =
		crate::cli_runtime::build_release_request_result(false, &release_request, || {
			Ok(monochange_core::SourceChangeRequestOutcome {
				provider: monochange_core::SourceProvider::GitHub,
				repository: "ifiokjr/monochange".to_string(),
				number: 7,
				head_branch: "release/v1.2.3".to_string(),
				operation: monochange_core::SourceChangeRequestOperation::Updated,
				url: Some("https://example.com/pr/7".to_string()),
			})
		})
		.unwrap_or_else(|error| panic!("render release request result: {error}"));
	assert_eq!(
		release_request_result,
		"ifiokjr/monochange #7 (updated) via github"
	);

	let issue_comment_plans = vec![monochange_github::GitHubIssueCommentPlan {
		repository: "ifiokjr/monochange".to_string(),
		issue_id: "#7".to_string(),
		issue_url: Some("https://example.com/issues/7".to_string()),
		body: "released".to_string(),
		close: false,
	}];
	let issue_comment_results =
		crate::cli_runtime::build_issue_comment_results(false, &issue_comment_plans, || {
			Ok(vec![
				monochange_github::GitHubIssueCommentOutcome {
					repository: "ifiokjr/monochange".to_string(),
					issue_id: "#7".to_string(),
					operation: monochange_github::GitHubIssueCommentOperation::Created,
					url: Some("https://example.com/issues/7#comment-1".to_string()),
				},
				monochange_github::GitHubIssueCommentOutcome {
					repository: "ifiokjr/monochange".to_string(),
					issue_id: "#8".to_string(),
					operation: monochange_github::GitHubIssueCommentOperation::SkippedExisting,
					url: Some("https://example.com/issues/8#comment-2".to_string()),
				},
			])
		})
		.unwrap_or_else(|error| panic!("render issue comment results: {error}"));
	assert_eq!(
		issue_comment_results,
		vec![
			"ifiokjr/monochange #7 (created)".to_string(),
			"ifiokjr/monochange #8 (skipped_existing)".to_string(),
		]
	);
}

#[test]
fn quiet_builtin_commands_return_empty_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	let init_output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("--quiet"),
			OsString::from("init"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet init output: {error}"));
	assert!(init_output.is_empty());
	assert!(root.join("monochange.toml").exists());

	let analyze_error = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("--quiet"),
			OsString::from("analyze"),
			OsString::from("--package"),
			OsString::from("core"),
		],
	)
	.expect_err("quiet analyze should still validate the requested package");
	assert!(analyze_error.to_string().contains("core"));

	let mcp_output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("--quiet"),
			OsString::from("mcp"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet mcp output: {error}"));
	assert!(mcp_output.is_empty());

	let check_output = run_cli(
		root,
		[
			OsString::from("monochange"),
			OsString::from("--quiet"),
			OsString::from("check"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet check output: {error}"));
	assert!(check_output.is_empty());
}

#[test]
fn inferred_retarget_source_configuration_infers_from_release_record_provider() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let inferred = inferred_retarget_source_configuration(None, &discovery, true)
		.unwrap_or_else(|| panic!("expected inferred source"));
	assert_eq!(inferred.provider, monochange_core::SourceProvider::GitHub);
	assert_eq!(inferred.owner, "ifiokjr");
	assert_eq!(inferred.repo, "monochange");
}

#[test]
fn render_retarget_release_report_handles_provider_sync_variants() {
	let mut report = sample_retarget_release_report();
	report.sync_provider = true;
	report.provider_results = vec![monochange_core::RetargetProviderResult {
		provider: monochange_core::SourceProvider::GitHub,
		tag_name: "v1.2.3".to_string(),
		target_commit: "def5678901234".to_string(),
		operation: monochange_core::RetargetProviderOperation::Synced,
		url: None,
		message: None,
	}];
	let rendered = render_retarget_release_report(&report);
	insta::assert_snapshot!("retarget_release_report_dry_run", rendered);

	let mut no_provider_report = sample_retarget_release_report();
	no_provider_report.sync_provider = true;
	no_provider_report.provider_results = Vec::new();
	no_provider_report.git_tag_results = Vec::new();
	let rendered = render_retarget_release_report(&no_provider_report);
	assert!(
		rendered.contains("Provider       nothing to sync"),
		"{rendered}"
	);
	assert!(!rendered.contains("Tags ("), "{rendered}");

	let mut applied = sample_retarget_release_report();
	applied.dry_run = false;
	applied.force = true;
	applied.is_descendant = false;
	applied.git_tag_results = [
		monochange_core::RetargetOperation::Moved,
		monochange_core::RetargetOperation::AlreadyUpToDate,
		monochange_core::RetargetOperation::Skipped,
		monochange_core::RetargetOperation::Failed,
	]
	.into_iter()
	.map(|operation| {
		monochange_core::RetargetTagResult {
			tag_name: format!("v1.2.3-{operation:?}"),
			from_commit: "abc1234567890".to_string(),
			to_commit: "def5678901234".to_string(),
			operation,
			message: (operation == monochange_core::RetargetOperation::Failed)
				.then(|| "tag is protected".to_string()),
		}
	})
	.collect();
	insta::assert_snapshot!(
		"retarget_release_report_with_failures",
		render_retarget_release_report(&applied)
	);

	applied
		.git_tag_results
		.retain(|tag_result| tag_result.operation != monochange_core::RetargetOperation::Failed);
	assert!(
		render_retarget_release_report(&applied)
			.lines()
			.last()
			.unwrap_or_default()
			.starts_with("✔ Moved 1 release tag to def5678"),
		"{}",
		render_retarget_release_report(&applied)
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn plan_release_retarget_marks_unsupported_provider_sync_in_dry_run() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);

	let mut record = sample_release_record_for_retarget();
	record.provider = Some(monochange_core::ReleaseRecordProvider {
		kind: monochange_core::SourceProvider::GitLab,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
	});
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};

	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, Some(&source))
			.await
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	let provider_update = plan
		.provider_updates
		.first()
		.unwrap_or_else(|| panic!("expected provider update"));
	assert_eq!(
		provider_update.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
	assert!(
		provider_update
			.message
			.as_deref()
			.unwrap_or("")
			.contains("gitlab")
	);

	let result = crate::execute_release_retarget(root, Some(&source), &plan)
		.await
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(
		result
			.provider_results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn retarget_release_succeeds_end_to_end_without_provider_sync() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let remote = root.join("remote.git");
	git_in_dir(
		root,
		&["init", "--bare", remote.to_str().unwrap_or("remote.git")],
	);
	let repo = root.join("repo");
	fs::create_dir_all(&repo).unwrap_or_else(|error| panic!("create repo dir: {error}"));
	init_git_repo(&repo);
	git_in_temp_repo(
		&repo,
		&[
			"remote",
			"add",
			"origin",
			remote.to_str().unwrap_or("remote.git"),
		],
	);

	fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	git_in_temp_repo(&repo, &["tag", "v1.2.3"]);
	git_in_temp_repo(
		&repo,
		&[
			"push",
			"origin",
			"HEAD",
			"refs/tags/v1.2.3:refs/tags/v1.2.3",
		],
	);
	fs::write(repo.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "second"]);
	let target_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let result =
		crate::retarget_release(&repo, &discovery, &target_commit, false, false, false, None)
			.await
			.unwrap_or_else(|error| panic!("retarget release: {error}"));
	assert_eq!(result.git_tag_results.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn sync_retargeted_provider_releases_reports_unsupported_provider_in_dry_run() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Gitea,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	let updates = vec![monochange_core::RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: monochange_core::RetargetOperation::Moved,
		message: None,
	}];
	let results = crate::release_record::sync_retargeted_provider_releases(&source, &updates, true)
		.await
		.unwrap_or_else(|error| panic!("expected dry-run provider results: {error}"));
	assert_eq!(results.len(), 1);
	assert_eq!(
		results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
}

fn sample_retarget_release_report() -> crate::RetargetReleaseReport {
	crate::RetargetReleaseReport {
		from: "v1.2.3".to_string(),
		target: "HEAD".to_string(),
		resolved_from_commit: "abc1234567890".to_string(),
		record_commit: "abc1234567890".to_string(),
		target_commit: "def5678901234".to_string(),
		distance: 1,
		is_descendant: true,
		force: false,
		dry_run: true,
		sync_provider: false,
		tags: vec!["v1.2.3".to_string()],
		git_tag_results: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "abc1234567890".to_string(),
			to_commit: "def5678901234".to_string(),
			operation: monochange_core::RetargetOperation::Planned,
			message: None,
		}],
		provider_results: Vec::new(),
		status: "dry_run".to_string(),
	}
}

#[test]
fn read_cached_document_reports_python_error_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest_path = tempdir.path().join("pyproject.toml");
	let lock_path = tempdir.path().join("uv.lock");
	let unsupported_path = tempdir.path().join("unknown.txt");

	fs::write(&manifest_path, [0xff, 0xfe])
		.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&manifest_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid UTF-8 manifest error"));
	assert!(error.to_string().contains("is not valid UTF-8"));

	fs::write(&lock_path, [0xff, 0xfe]).unwrap_or_else(|error| panic!("write lock: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&lock_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid UTF-8 lock error"));
	assert!(error.to_string().contains("is not valid UTF-8"));

	fs::write(&manifest_path, "[project\n")
		.unwrap_or_else(|error| panic!("write invalid manifest: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&manifest_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid TOML manifest error"));
	assert!(error.to_string().contains("failed to parse"));

	fs::write(&unsupported_path, "text")
		.unwrap_or_else(|error| panic!("write unsupported: {error}"));
	let error = crate::read_cached_document(
		&mut BTreeMap::new(),
		&unsupported_path,
		monochange_core::EcosystemType::Python,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported Python versioned file error"));
	assert!(error.to_string().contains("python"));
}

#[test]
fn build_manifest_updates_report_parse_and_io_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let npm_path = tempdir.path().join("package.json");
	let dart_path = tempdir.path().join("pubspec.yaml");
	fs::write(&npm_path, "{").unwrap_or_else(|error| panic!("write package.json: {error}"));
	fs::write(&dart_path, ": bad").unwrap_or_else(|error| panic!("write pubspec.yaml: {error}"));
	let npm = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"web",
		npm_path,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let deno_missing = monochange_core::PackageRecord::new(
		Ecosystem::Deno,
		"tool",
		tempdir.path().join("missing/deno.json"),
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let deno_invalid_path = tempdir.path().join("invalid/deno.json");

	fs::create_dir_all(deno_invalid_path.parent().expect("deno manifest parent"))
		.unwrap_or_else(|error| panic!("create invalid deno dir: {error}"));
	fs::write(&deno_invalid_path, "{")
		.unwrap_or_else(|error| panic!("write invalid deno manifest: {error}"));
	let deno_invalid = monochange_core::PackageRecord::new(
		Ecosystem::Deno,
		"tool-invalid",
		deno_invalid_path,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let dart = monochange_core::PackageRecord::new(
		Ecosystem::Dart,
		"mobile",
		dart_path,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let dart_missing = monochange_core::PackageRecord::new(
		Ecosystem::Dart,
		"mobile-missing",
		tempdir.path().join("missing/pubspec.yaml"),
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![
			npm.id.clone(),
			deno_missing.id.clone(),
			deno_invalid.id.clone(),
			dart.id.clone(),
			dart_missing.id.clone(),
		]
		.into_iter()
		.map(|package_id| {
			monochange_core::ReleaseDecision {
				package_id,
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.1.0")
						.unwrap_or_else(|error| panic!("planned version: {error}")),
				),
				group_id: None,
				reasons: vec!["release".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}
		})
		.collect(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let npm_error = crate::build_npm_manifest_updates(std::slice::from_ref(&npm), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected npm parse error"));
	assert!(
		npm_error.to_string().contains("failed to parse"),
		"error: {npm_error}"
	);
	let deno_error = crate::build_deno_manifest_updates(std::slice::from_ref(&deno_missing), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected deno io error"));
	assert!(
		deno_error.to_string().contains("failed to read"),
		"error: {deno_error}"
	);
	let deno_parse_error =
		crate::build_deno_manifest_updates(std::slice::from_ref(&deno_invalid), &plan)
			.err()
			.unwrap_or_else(|| panic!("expected deno parse error"));
	assert!(
		deno_parse_error.to_string().contains("failed to parse"),
		"error: {deno_parse_error}"
	);
	let dart_error = crate::build_dart_manifest_updates(std::slice::from_ref(&dart), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected dart parse error"));
	assert!(
		dart_error.to_string().contains("failed to parse"),
		"error: {dart_error}"
	);
	let dart_read_error =
		crate::build_dart_manifest_updates(std::slice::from_ref(&dart_missing), &plan)
			.err()
			.unwrap_or_else(|| panic!("expected dart read error"));
	assert!(
		dart_read_error.to_string().contains("failed to read"),
		"error: {dart_read_error}"
	);

	let cargo_missing_dir = tempdir.path().join("cargo-missing-dir");
	fs::create_dir_all(&cargo_missing_dir)
		.unwrap_or_else(|error| panic!("create cargo missing dir: {error}"));
	let cargo_missing = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-missing",
		cargo_missing_dir,
		tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_invalid_path = tempdir.path().join("invalid-package/Cargo.toml");
	fs::create_dir_all(cargo_invalid_path.parent().expect("cargo package parent"))
		.unwrap_or_else(|error| panic!("create invalid cargo package dir: {error}"));
	fs::write(&cargo_invalid_path, "{")
		.unwrap_or_else(|error| panic!("write invalid cargo manifest: {error}"));
	let cargo_invalid = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-invalid",
		cargo_invalid_path,
		tempdir.path().join("cargo-invalid-workspace"),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_workspace_dir = tempdir.path().join("cargo-workspace-dir");

	fs::create_dir_all(cargo_workspace_dir.join("crates/core"))
		.unwrap_or_else(|error| panic!("create cargo workspace package dir: {error}"));
	fs::write(
		cargo_workspace_dir.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo workspace package manifest: {error}"));
	fs::create_dir_all(cargo_workspace_dir.join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("create workspace cargo root dir: {error}"));
	let cargo_workspace_read = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-workspace-read",
		cargo_workspace_dir.join("crates/core/Cargo.toml"),
		cargo_workspace_dir.clone(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_workspace_invalid = tempdir.path().join("cargo-workspace-invalid");
	fs::create_dir_all(cargo_workspace_invalid.join("crates/core"))
		.unwrap_or_else(|error| panic!("create cargo workspace invalid dir: {error}"));
	fs::write(
		cargo_workspace_invalid.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write valid package manifest: {error}"));
	fs::write(cargo_workspace_invalid.join("Cargo.toml"), "{")
		.unwrap_or_else(|error| panic!("write invalid workspace manifest: {error}"));
	let cargo_workspace_parse = monochange_core::PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-workspace-parse",
		cargo_workspace_invalid.join("crates/core/Cargo.toml"),
		cargo_workspace_invalid,
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let cargo_plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: vec![
			cargo_missing.id.clone(),
			cargo_invalid.id.clone(),
			cargo_workspace_read.id.clone(),
			cargo_workspace_parse.id.clone(),
		]
		.into_iter()
		.map(|package_id| {
			monochange_core::ReleaseDecision {
				package_id,
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.1.0")
						.unwrap_or_else(|error| panic!("planned version: {error}")),
				),
				group_id: None,
				reasons: vec!["release".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}
		})
		.collect(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let cargo_read_error =
		crate::build_cargo_manifest_updates(std::slice::from_ref(&cargo_missing), &cargo_plan)
			.err()
			.unwrap_or_else(|| panic!("expected cargo read error"));
	assert!(
		cargo_read_error.to_string().contains("failed to read"),
		"error: {cargo_read_error}"
	);
	let cargo_parse_error =
		crate::build_cargo_manifest_updates(std::slice::from_ref(&cargo_invalid), &cargo_plan)
			.err()
			.unwrap_or_else(|| panic!("expected cargo parse error"));
	assert!(
		cargo_parse_error.to_string().contains("failed to parse"),
		"error: {cargo_parse_error}"
	);
	let cargo_workspace_read_error = crate::build_cargo_manifest_updates(
		std::slice::from_ref(&cargo_workspace_read),
		&cargo_plan,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cargo workspace read error"));
	assert!(
		cargo_workspace_read_error
			.to_string()
			.contains("failed to read"),
		"error: {cargo_workspace_read_error}"
	);
	let cargo_workspace_parse_error = crate::build_cargo_manifest_updates(
		std::slice::from_ref(&cargo_workspace_parse),
		&cargo_plan,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cargo workspace parse error"));
	assert!(
		cargo_workspace_parse_error
			.to_string()
			.contains("failed to parse"),
		"error: {cargo_workspace_parse_error}"
	);

	let unreleased_plan = monochange_core::ReleasePlan {
		workspace_root: tempdir.path().to_path_buf(),
		decisions: Vec::new(),
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	assert!(
		crate::build_deno_manifest_updates(&[deno_missing], &unreleased_plan)
			.unwrap_or_else(|error| panic!("unreleased deno updates: {error}"))
			.is_empty()
	);
}

#[test]
fn apply_versioned_file_definition_reports_manifest_parse_errors_for_text_updaters() {
	let configuration = versioned_test_configuration();
	for (file_name, ecosystem_type, contents) in [
		("Cargo.toml", monochange_core::EcosystemType::Cargo, "{"),
		("package.json", monochange_core::EcosystemType::Npm, "{"),
		("deno.json", monochange_core::EcosystemType::Deno, "{"),
		(
			"pubspec.yaml",
			monochange_core::EcosystemType::Dart,
			": bad",
		),
	] {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::write(&path, contents).unwrap_or_else(|error| panic!("write {file_name}: {error}"));
		let context = versioned_test_context(
			&configuration,
			BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
			&[],
		);
		let definition = monochange_core::VersionedFileDefinition {
			path: file_name.to_string(),
			ecosystem_type: Some(ecosystem_type),
			format: None,
			prefix: None,
			fields: None,
			name: None,
			missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
			regex: None,
			value_template: None,
		};
		let error = apply_versioned_file_definition(
			tempdir.path(),
			&mut BTreeMap::new(),
			&definition,
			"2.0.0",
			None,
			&["core".to_string()],
			&context,
		)
		.err()
		.unwrap_or_else(|| panic!("expected parse error for {file_name}"));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
	}

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = tempdir.path().join("Cargo.toml");
	fs::write(&path, "[package]\nname = \"core\"\nversion = \"1.0.0\"\n")
		.unwrap_or_else(|error| panic!("write cached cargo manifest path: {error}"));
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let definition = monochange_core::VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Cargo),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = apply_versioned_file_definition(
		tempdir.path(),
		&mut BTreeMap::from([(path, crate::CachedDocument::Text("{".to_string()))]),
		&definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cached cargo parse error"));
	assert!(
		error.to_string().contains("failed to parse"),
		"error: {error}"
	);

	let pnpm_tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let pnpm_path = pnpm_tempdir.path().join("pnpm-lock.yaml");
	fs::write(&pnpm_path, "lockfileVersion: '9.0'\n")
		.unwrap_or_else(|error| panic!("write cached pnpm lock path: {error}"));
	let pnpm_definition = monochange_core::VersionedFileDefinition {
		path: "pnpm-lock.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = apply_versioned_file_definition(
		pnpm_tempdir.path(),
		&mut BTreeMap::from([(pnpm_path, crate::CachedDocument::Text(": bad".to_string()))]),
		&pnpm_definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cached pnpm parse error"));
	assert!(
		error.to_string().contains("failed to parse"),
		"error: {error}"
	);

	let cached_dart_tempdir =
		tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let path = cached_dart_tempdir.path().join("pubspec.yaml");
	fs::write(&path, "name: app\nversion: 1.0.0\n")
		.unwrap_or_else(|error| panic!("write cached dart manifest path: {error}"));
	let definition = monochange_core::VersionedFileDefinition {
		path: "pubspec.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Dart),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = apply_versioned_file_definition(
		cached_dart_tempdir.path(),
		&mut BTreeMap::from([(path, crate::CachedDocument::Text(": bad".to_string()))]),
		&definition,
		"2.0.0",
		None,
		&["core".to_string()],
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected cached dart parse error"));
	assert!(
		error.to_string().contains("failed to parse"),
		"error: {error}"
	);
}

#[test]
fn read_cached_document_rejects_invalid_utf8_in_text_formats() {
	let cases = [
		(
			"versioned-file-invalid-utf8/invalid-cargo.toml",
			"Cargo.toml",
			monochange_core::EcosystemType::Cargo,
		),
		(
			"versioned-file-invalid-utf8/invalid-package-lock.json",
			"package-lock.json",
			monochange_core::EcosystemType::Npm,
		),
		(
			"versioned-file-invalid-utf8/invalid-pnpm-lock.yaml",
			"pnpm-lock.yaml",
			monochange_core::EcosystemType::Npm,
		),
		(
			"versioned-file-invalid-utf8/invalid-bun.lock",
			"bun.lock",
			monochange_core::EcosystemType::Npm,
		),
		(
			"versioned-file-invalid-utf8/invalid-yarn.lock",
			"yarn.lock",
			monochange_core::EcosystemType::Npm,
		),
	];

	for (fixture, file_name, ecosystem) in cases {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::copy(fixture_path(fixture), &path)
			.unwrap_or_else(|error| panic!("copy fixture {fixture}: {error}"));
		let error = crate::read_cached_document(&mut BTreeMap::new(), &path, ecosystem)
			.err()
			.unwrap_or_else(|| panic!("expected utf8 error for {}", path.display()));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
		assert!(error.to_string().contains("as text"), "error: {error}");
	}
}

#[test]
fn read_cached_document_rejects_invalid_utf8_manifest_text_formats() {
	for (file_name, ecosystem) in [
		("package.json", monochange_core::EcosystemType::Npm),
		("deno.json", monochange_core::EcosystemType::Deno),
		("pubspec.yaml", monochange_core::EcosystemType::Dart),
	] {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let path = tempdir.path().join(file_name);
		fs::write(&path, [0xff_u8, 0xfe_u8])
			.unwrap_or_else(|error| panic!("write invalid utf8 {file_name}: {error}"));
		let error = crate::read_cached_document(&mut BTreeMap::new(), &path, ecosystem)
			.err()
			.unwrap_or_else(|| panic!("expected utf8 error for {}", path.display()));
		assert!(
			error.to_string().contains("failed to parse"),
			"error: {error}"
		);
		assert!(error.to_string().contains("as text"), "error: {error}");
	}
}

#[test]
fn apply_versioned_file_definition_updates_npm_manifest_and_lock_variants() {
	let configuration = versioned_test_configuration();

	let manifest_tempdir = setup_fixture("versioned-file-updates/npm-manifest");
	let manifest_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let manifest_definition = monochange_core::VersionedFileDefinition {
		path: "package.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let manifest_dep_names = vec!["core".to_string()];
	let mut manifest_updates = BTreeMap::new();
	apply_versioned_file_definition(
		manifest_tempdir.path(),
		&mut manifest_updates,
		&manifest_definition,
		"2.0.0",
		None,
		&manifest_dep_names,
		&manifest_context,
	)
	.unwrap_or_else(|error| panic!("npm manifest update: {error}"));
	let manifest_path = manifest_tempdir.path().join("package.json");
	let manifest_document = manifest_updates
		.remove(&manifest_path)
		.unwrap_or_else(|| panic!("expected npm manifest update"));

	assert!(matches!(
		manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("\"core\": \"^2.0.0\"")
	));

	let package_lock_tempdir = setup_fixture("monochange/npm-lock-release");
	let package = monochange_core::PackageRecord::new(
		Ecosystem::Npm,
		"app",
		package_lock_tempdir
			.path()
			.join("packages/app/package.json"),
		package_lock_tempdir.path().to_path_buf(),
		Some(Version::parse("1.0.0").unwrap_or_else(|error| panic!("version: {error}"))),
		monochange_core::PublishState::Public,
	);
	let package_lock_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("app".to_string(), "2.0.0".to_string())]),
		std::slice::from_ref(&package),
	);
	let package_lock_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/package-lock.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let package_lock_dep_names = vec!["app".to_string()];
	let mut package_lock_updates = BTreeMap::new();
	apply_versioned_file_definition(
		package_lock_tempdir.path(),
		&mut package_lock_updates,
		&package_lock_definition,
		"2.0.0",
		None,
		&package_lock_dep_names,
		&package_lock_context,
	)
	.unwrap_or_else(|error| panic!("package lock update: {error}"));
	let package_lock_path = package_lock_tempdir
		.path()
		.join("packages/app/package-lock.json");

	let package_lock_document = package_lock_updates
		.remove(&package_lock_path)
		.unwrap_or_else(|| panic!("expected package-lock update"));
	assert!(matches!(
		package_lock_document,
		crate::CachedDocument::Json(value)
			if value["version"] == serde_json::Value::String("2.0.0".to_string())
				&& value["packages"][""]["version"] == serde_json::Value::String("2.0.0".to_string())
				&& value["dependencies"]["app"]["version"] == serde_json::Value::String("2.0.0".to_string())
	));

	let pnpm_tempdir = setup_fixture("versioned-file-updates/pnpm-lock");
	let pnpm_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let pnpm_definition = monochange_core::VersionedFileDefinition {
		path: "pnpm-lock.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let pnpm_dep_names = vec!["core".to_string()];
	let mut pnpm_updates = BTreeMap::new();
	apply_versioned_file_definition(
		pnpm_tempdir.path(),
		&mut pnpm_updates,
		&pnpm_definition,
		"2.0.0",
		None,
		&pnpm_dep_names,
		&pnpm_context,
	)
	.unwrap_or_else(|error| panic!("pnpm lock update: {error}"));
	let pnpm_path = pnpm_tempdir.path().join("pnpm-lock.yaml");
	let pnpm_document = pnpm_updates
		.remove(&pnpm_path)
		.unwrap_or_else(|| panic!("expected pnpm lock update"));

	assert!(matches!(
		pnpm_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("core: 2.0.0")
				&& contents.contains("lockfileVersion: '9.0'")
	));

	let yarn_tempdir = setup_fixture("versioned-file-updates/yarn-lock");
	let yarn_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let yarn_definition = monochange_core::VersionedFileDefinition {
		path: "yarn.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let yarn_dep_names = vec!["core".to_string()];
	let mut yarn_updates = BTreeMap::new();
	apply_versioned_file_definition(
		yarn_tempdir.path(),
		&mut yarn_updates,
		&yarn_definition,
		"2.0.0",
		None,
		&yarn_dep_names,
		&yarn_context,
	)
	.unwrap_or_else(|error| panic!("yarn lock update: {error}"));
	let yarn_path = yarn_tempdir.path().join("yarn.lock");
	let yarn_document = yarn_updates
		.remove(&yarn_path)
		.unwrap_or_else(|| panic!("expected yarn lock update"));

	assert!(matches!(
		yarn_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("version \"2.0.0\"")
				&& contents.contains("# yarn lockfile v1")
	));

	let bun_tempdir = setup_fixture("npm/bun-text-lock");
	let bun_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("left-pad".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let bun_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/bun.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Npm),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let bun_dep_names = vec!["left-pad".to_string()];
	let mut bun_updates = BTreeMap::new();
	apply_versioned_file_definition(
		bun_tempdir.path(),
		&mut bun_updates,
		&bun_definition,
		"2.0.0",
		None,
		&bun_dep_names,
		&bun_context,
	)
	.unwrap_or_else(|error| panic!("bun lock update: {error}"));
	let bun_path = bun_tempdir.path().join("packages/app/bun.lock");
	let bun_document = bun_updates
		.remove(&bun_path)
		.unwrap_or_else(|| panic!("expected bun lock update"));

	assert!(matches!(
		bun_document,
		crate::CachedDocument::Text(contents) if contents.contains("\"left-pad\": \"2.0.0\"")
	));
}

#[test]
fn apply_versioned_file_definition_updates_python_manifest_and_lock_variants() {
	let configuration = versioned_test_configuration();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let manifest_path = tempdir.path().join("pyproject.toml");
	fs::write(
		&manifest_path,
		r#"[project]
name = "python-app"
version = "1.0.0"
dependencies = ["python-core>=1.0.0"]
"#,
	)
	.unwrap_or_else(|error| panic!("write pyproject: {error}"));
	let lock_path = tempdir.path().join("uv.lock");
	fs::write(&lock_path, "version = 1\n").unwrap_or_else(|error| panic!("write lock: {error}"));

	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("python-core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let manifest_definition = monochange_core::VersionedFileDefinition {
		path: "pyproject.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let dep_names = vec!["python-core".to_string()];
	let mut updates = BTreeMap::new();
	apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&manifest_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.unwrap_or_else(|error| panic!("python manifest update: {error}"));
	let manifest_document = updates
		.remove(&manifest_path)
		.unwrap_or_else(|| panic!("expected python manifest update"));
	assert!(matches!(
		manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("version = \"1.0.0\"")
				&& contents.contains("python-core>=2.0.0")
	));

	let lock_definition = monochange_core::VersionedFileDefinition {
		path: "uv.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&lock_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.unwrap_or_else(|error| panic!("python lock update: {error}"));
	let lock_document = updates
		.remove(&lock_path)
		.unwrap_or_else(|| panic!("expected python lock update"));
	assert!(matches!(
		lock_document,
		crate::CachedDocument::Text(contents) if contents == "version = 1\n"
	));
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_versioned_file_definition_reports_python_error_paths() {
	let configuration = versioned_test_configuration();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let context = versioned_test_context(
		&configuration,
		BTreeMap::from([("python-core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dep_names = vec!["python-core".to_string()];
	let mut updates = BTreeMap::new();

	fs::write(tempdir.path().join("unknown.txt"), "text")
		.unwrap_or_else(|error| panic!("write unsupported: {error}"));
	let unsupported_definition = monochange_core::VersionedFileDefinition {
		path: "unknown.txt".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let error = apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&unsupported_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported python path error"));
	assert!(error.to_string().contains("python"));

	let manifest_path = tempdir.path().join("pyproject.toml");
	fs::write(
		&manifest_path,
		"[project]\nname = \"python-core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let manifest_definition = monochange_core::VersionedFileDefinition {
		path: "pyproject.toml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Python),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	updates.insert(
		manifest_path,
		crate::CachedDocument::Text("[project\n".to_string()),
	);
	let error = apply_versioned_file_definition(
		tempdir.path(),
		&mut updates,
		&manifest_definition,
		"2.0.0",
		None,
		&dep_names,
		&context,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid python manifest error"));
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn apply_versioned_file_definition_updates_deno_and_dart_variants() {
	let configuration = versioned_test_configuration();

	let deno_manifest_tempdir = setup_fixture("versioned-file-updates/deno-manifest");
	let deno_manifest_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("core".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let deno_manifest_definition = monochange_core::VersionedFileDefinition {
		path: "deno.json".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Deno),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let deno_manifest_dep_names = vec!["core".to_string()];
	let mut deno_manifest_updates = BTreeMap::new();
	apply_versioned_file_definition(
		deno_manifest_tempdir.path(),
		&mut deno_manifest_updates,
		&deno_manifest_definition,
		"2.0.0",
		None,
		&deno_manifest_dep_names,
		&deno_manifest_context,
	)
	.unwrap_or_else(|error| panic!("deno manifest update: {error}"));
	let deno_manifest_path = deno_manifest_tempdir.path().join("deno.json");
	let deno_manifest_document = deno_manifest_updates
		.remove(&deno_manifest_path)
		.unwrap_or_else(|| panic!("expected deno manifest update"));

	assert!(matches!(
		deno_manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("\"core\": \"^2.0.0\"")
	));

	let deno_lock_tempdir = setup_fixture("monochange/deno-lock-release");
	let deno_lock_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("app".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let deno_lock_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/deno.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Deno),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let deno_lock_dep_names = vec!["app".to_string()];
	let mut deno_lock_updates = BTreeMap::new();
	apply_versioned_file_definition(
		deno_lock_tempdir.path(),
		&mut deno_lock_updates,
		&deno_lock_definition,
		"2.0.0",
		None,
		&deno_lock_dep_names,
		&deno_lock_context,
	)
	.unwrap_or_else(|error| panic!("deno lock update: {error}"));
	let deno_lock_path = deno_lock_tempdir.path().join("packages/app/deno.lock");
	let deno_lock_document = deno_lock_updates
		.remove(&deno_lock_path)
		.unwrap_or_else(|| panic!("expected deno lock update"));

	assert!(matches!(
		deno_lock_document,
		crate::CachedDocument::Json(value) if value.to_string().contains("npm:app@2.0.0")
	));

	let dart_manifest_tempdir = setup_fixture("dart/workspace-pattern-warnings");
	let dart_manifest_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("shared".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dart_manifest_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/pubspec.yaml".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Dart),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let dart_manifest_dep_names = vec!["shared".to_string()];
	let mut dart_manifest_updates = BTreeMap::new();
	apply_versioned_file_definition(
		dart_manifest_tempdir.path(),
		&mut dart_manifest_updates,
		&dart_manifest_definition,
		"2.0.0",
		None,
		&dart_manifest_dep_names,
		&dart_manifest_context,
	)
	.unwrap_or_else(|error| panic!("dart manifest update: {error}"));
	let dart_manifest_path = dart_manifest_tempdir
		.path()
		.join("packages/app/pubspec.yaml");
	let dart_manifest_document = dart_manifest_updates
		.remove(&dart_manifest_path)
		.unwrap_or_else(|| panic!("expected dart manifest update"));

	assert!(matches!(
		dart_manifest_document,
		crate::CachedDocument::Text(contents)
			if contents.contains("shared: ^2.0.0")
	));

	let dart_lock_tempdir = setup_fixture("dart/manifest-lockfile-workspace");
	let dart_lock_context = versioned_test_context(
		&configuration,
		BTreeMap::from([("nested_dart_app".to_string(), "2.0.0".to_string())]),
		&[],
	);
	let dart_lock_definition = monochange_core::VersionedFileDefinition {
		path: "packages/app/pubspec.lock".to_string(),
		ecosystem_type: Some(monochange_core::EcosystemType::Dart),
		format: None,
		prefix: None,
		fields: None,
		name: None,
		missing_field_behavior: monochange_core::MissingFieldBehavior::default(),
		regex: None,
		value_template: None,
	};
	let dart_lock_dep_names = vec!["nested_dart_app".to_string()];
	let mut dart_lock_updates = BTreeMap::new();
	apply_versioned_file_definition(
		dart_lock_tempdir.path(),
		&mut dart_lock_updates,
		&dart_lock_definition,
		"2.0.0",
		None,
		&dart_lock_dep_names,
		&dart_lock_context,
	)
	.unwrap_or_else(|error| panic!("dart lock update: {error}"));
	let dart_lock_path = dart_lock_tempdir.path().join("packages/app/pubspec.lock");
	let dart_lock_document = dart_lock_updates
		.remove(&dart_lock_path)
		.unwrap_or_else(|| panic!("expected dart lock update"));

	assert!(matches!(
		dart_lock_document,
		crate::CachedDocument::Yaml(mapping)
			if mapping
				.get(serde_yaml_ng::Value::String("packages".to_string()))
				.and_then(serde_yaml_ng::Value::as_mapping)
				.and_then(|packages| packages.get(serde_yaml_ng::Value::String("nested_dart_app".to_string())))
				.and_then(serde_yaml_ng::Value::as_mapping)
				.and_then(|entry| entry.get(serde_yaml_ng::Value::String("version".to_string())))
				.and_then(serde_yaml_ng::Value::as_str)
				== Some("2.0.0")
	));
}

fn versioned_test_configuration() -> monochange_core::WorkspaceConfiguration {
	load_workspace_configuration(&fixture_path("monochange/release-base"))
		.unwrap_or_else(|error| panic!("configuration: {error}"))
}

fn versioned_test_context<'a>(
	configuration: &'a monochange_core::WorkspaceConfiguration,
	released_versions_by_native_name: BTreeMap<String, String>,
	packages: &'a [monochange_core::PackageRecord],
) -> crate::VersionedFileUpdateContext<'a> {
	crate::VersionedFileUpdateContext {
		package_by_config_id: packages
			.iter()
			.filter_map(|package| {
				package
					.metadata
					.get("config_id")
					.map(|config_id| (config_id.as_str(), package))
			})
			.collect(),
		package_by_native_name: packages
			.iter()
			.map(|package| (package.name.as_str(), package))
			.collect(),
		current_versions_by_native_name: packages
			.iter()
			.filter_map(|package| {
				package
					.current_version
					.as_ref()
					.map(|version| (package.name.clone(), version.to_string()))
			})
			.collect(),
		released_versions_by_native_name,
		configuration,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		release_values: BTreeMap::new(),
	}
}

fn write_blank_monochange_config(root: &Path) {
	fs::write(root.join("monochange.toml"), "")
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
}

fn text_format_step_inputs() -> BTreeMap<String, monochange_core::CliStepInputValue> {
	BTreeMap::from([(
		"format".to_string(),
		monochange_core::CliStepInputValue::String("text".to_string()),
	)])
}

fn inherited_format_package_step_inputs() -> BTreeMap<String, monochange_core::CliStepInputValue> {
	BTreeMap::from([
		(
			"format".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
		(
			"package".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
	])
}

fn inherited_format_package_output_step_inputs()
-> BTreeMap<String, monochange_core::CliStepInputValue> {
	BTreeMap::from([
		(
			"format".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
		(
			"package".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
		(
			"output".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
	])
}

fn inherited_publish_plan_step_inputs() -> BTreeMap<String, monochange_core::CliStepInputValue> {
	BTreeMap::from([
		(
			"format".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
		(
			"mode".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
		(
			"package".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
		(
			"readiness".to_string(),
			monochange_core::CliStepInputValue::Inherited,
		),
	])
}

fn create_release_record_history(root: &Path) {
	create_release_record_commit(root);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "follow-up\n")
		.unwrap_or_else(|error| panic!("write follow-up file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "fix: follow-up release change"]);
}

fn create_release_record_commit(root: &Path) -> String {
	let record = sample_release_record_for_retarget();
	create_release_record_commit_from_record(root, &record)
}

fn create_release_record_commit_from_record(
	root: &Path,
	record: &monochange_core::ReleaseRecord,
) -> String {
	init_git_repo(root);
	write_blank_monochange_config(root);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "monochange.toml", "release.txt"]);
	let json = serde_json::to_string_pretty(record)
		.unwrap_or_else(|error| panic!("serialize release record: {error}"));
	let hash = {
		use std::collections::hash_map::DefaultHasher;
		use std::hash::Hasher;
		let mut hasher = DefaultHasher::new();
		for target in &record.release_targets {
			hasher.write(target.id.as_bytes());
			hasher.write(target.version.as_bytes());
		}

		format!("{:016x}", hasher.finish())
	};
	let dir = root.join(".monochange/releases").join(&hash);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create release record dir: {error}"));
	let path = dir.join("release.json");
	fs::write(&path, &json).unwrap_or_else(|error| panic!("write release record: {error}"));
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "chore(release): prepare release"]);
	git_output_in_temp_repo(root, &["rev-parse", "HEAD"])
}

fn sample_release_record_for_retarget() -> monochange_core::ReleaseRecord {
	monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		versions: BTreeMap::from([("sdk".to_string(), "1.2.3".to_string())]),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
			members: vec!["monochange".to_string()],
			floating_tags: Vec::new(),
		}],
		released_packages: vec!["monochange".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		changesets: Vec::new(),
		changelogs: Vec::new(),
		package_publications: Vec::new(),
		provider: Some(monochange_core::ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	}
}

#[test]
fn filter_group_release_note_change_handles_missing_context_and_direct_group_targets() {
	let planned_group = sample_planned_group();
	let group = sample_group_definition(GroupChangelogInclude::GroupOnly);

	assert!(
		filter_group_release_note_change(
			&sample_release_note_change(None),
			Some(&group),
			&planned_group,
			&BTreeMap::new(),
		)
		.is_none()
	);

	assert!(
		filter_group_release_note_change(
			&sample_release_note_change(Some(".changeset/missing.md")),
			Some(&group),
			&planned_group,
			&BTreeMap::new(),
		)
		.is_none()
	);

	let group_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/group.md"),
		vec![PreparedChangesetTarget {
			id: "sdk".to_string(),
			kind: ChangesetTargetKind::Group,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
			caused_by: Vec::new(),
		}],
	)]);
	let renamed = filter_group_release_note_change(
		&sample_release_note_change(Some(".changeset/group.md")),
		Some(&group),
		&planned_group,
		&group_changeset,
	)
	.unwrap_or_else(|| panic!("expected direct group note"));
	assert_eq!(renamed.package_name, "sdk");

	let outside_group_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/outside.md"),
		vec![PreparedChangesetTarget {
			id: "docs".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
			caused_by: Vec::new(),
		}],
	)]);
	assert!(
		filter_group_release_note_change(
			&sample_release_note_change(Some(".changeset/outside.md")),
			Some(&group),
			&planned_group,
			&outside_group_changeset,
		)
		.is_none()
	);
}

fn sample_release_note_change(source_path: Option<&str>) -> ReleaseNoteChange {
	ReleaseNoteChange {
		package_id: "core".to_string(),
		package_name: "core".to_string(),
		package_labels: Vec::new(),
		stream: "default".to_string(),
		source_path: source_path.map(str::to_string),
		summary: "add grouped note".to_string(),
		details: None,
		bump: BumpSeverity::Minor,
		change_type: None,
		context: None,
		changeset_path: None,
		change_owner: None,
		change_owner_link: None,
		review_request: None,
		review_request_link: None,
		introduced_commit: None,
		introduced_commit_link: None,
		last_updated_commit: None,
		last_updated_commit_link: None,
		related_issues: None,
		related_issue_links: None,
		closed_issues: None,
		closed_issue_links: None,
	}
}

fn sample_group_definition(include: GroupChangelogInclude) -> monochange_core::GroupDefinition {
	monochange_core::GroupDefinition {
		id: "sdk".to_string(),
		packages: vec!["core".to_string(), "app".to_string()],
		package_max_bumps: BTreeMap::new(),
		changelog: None,
		changelog_include: include,
		excluded_changelog_types: Vec::new(),
		bump_propagation: None,
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: false,
		release: false,
		version_format: VersionFormat::Namespaced,
		version_source: VersionSource::default(),
		initial_version: None,
		bump_ceiling: None,
		classification_enforced: None,

		floating_tags: Vec::new(),
	}
}

#[test]
fn skill_command_lists_every_bundled_topic_from_the_real_entry_point() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output = run_cli(
		tempdir.path(),
		[OsString::from("monochange"), OsString::from("skill")],
	)
	.unwrap_or_else(|error| panic!("list skill topics: {error}"));
	assert!(output.contains("monochange skill read <topic>"));
	assert!(output.contains("monochange skill install --dir <dir>"));
	assert!(output.contains("SKILL.md"));
	assert!(output.contains("skills/configuration.md"));
	assert!(output.contains("examples/quickstart.md"));
	assert!(output.contains("example-quickstart"));
}

#[test]
fn skill_command_reads_a_topic_verbatim_from_the_real_entry_point() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("skill"),
			OsString::from("read"),
			OsString::from("example-readme"),
		],
	)
	.unwrap_or_else(|error| panic!("read skill topic: {error}"));
	assert!(
		output.is_empty(),
		"raw documents are written straight to stdout without a second copy"
	);
}

#[test]
fn skill_command_rejects_an_unknown_topic() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("skill"),
			OsString::from("read"),
			OsString::from("anchor"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("unknown skill topic must fail"));
	assert!(error.to_string().contains("unknown skill topic `anchor`"));
	assert!(error.to_string().contains("configuration"));
}

#[test]
fn skill_command_installs_a_tree_and_refuses_to_clobber_without_force() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let destination = tempdir.path().join("skills").join("monochange");

	let install_args = || {
		[
			OsString::from("monochange"),
			OsString::from("skill"),
			OsString::from("install"),
			OsString::from("--dir"),
			destination.as_os_str().to_owned(),
		]
	};
	let summary = run_cli(tempdir.path(), install_args())
		.unwrap_or_else(|error| panic!("install skill: {error}"));
	assert!(summary.contains("Installed 18 files"));
	assert!(destination.join("SKILL.md").is_file());
	assert!(destination.join("skills/configuration.md").is_file());
	assert!(destination.join("examples/quickstart.md").is_file());

	let error = run_cli(tempdir.path(), install_args())
		.err()
		.unwrap_or_else(|| panic!("a second install must refuse to clobber"));
	assert!(error.to_string().contains("already contains a skill"));
	assert!(error.to_string().contains("--force"));

	let mut forced_args = install_args().to_vec();
	forced_args.push(OsString::from("--force"));
	let forced = run_cli(tempdir.path(), forced_args)
		.unwrap_or_else(|error| panic!("forced install: {error}"));
	assert!(forced.contains("Installed 18 files"));
}

#[test]
fn skill_command_reports_a_missing_install_destination() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("skill"),
			OsString::from("install"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("install without --dir must fail"));
	assert!(error.to_string().contains("monochange skill install --dir"));
}

#[test]
fn subagent_parsing_helpers_cover_defaults_deduplication_and_errors() {
	assert_eq!(
		crate::SubagentTarget::all(),
		vec![
			crate::SubagentTarget::Claude,
			crate::SubagentTarget::Vscode,
			crate::SubagentTarget::Copilot,
			crate::SubagentTarget::Pi,
			crate::SubagentTarget::Codex,
			crate::SubagentTarget::Cursor,
		]
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("claude"),
		Some(crate::SubagentTarget::Claude)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("vscode"),
		Some(crate::SubagentTarget::Vscode)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("copilot"),
		Some(crate::SubagentTarget::Copilot)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("pi"),
		Some(crate::SubagentTarget::Pi)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("codex"),
		Some(crate::SubagentTarget::Codex)
	);
	assert_eq!(
		crate::SubagentTarget::from_cli_value("cursor"),
		Some(crate::SubagentTarget::Cursor)
	);
	assert_eq!(crate::SubagentTarget::from_cli_value("unknown"), None);

	let json = String::from("json");
	assert_eq!(
		crate::parse_subagent_output_format_or_default(Some(&json)),
		crate::SubagentOutputFormat::Json
	);
	assert_eq!(
		crate::parse_subagent_output_format_or_default(None),
		crate::SubagentOutputFormat::Text
	);

	let claude = String::from("claude");
	let pi = String::from("pi");
	let targets = crate::parse_subagent_targets(Some(vec![&claude, &pi, &claude]))
		.unwrap_or_else(|error| panic!("parse subagent targets: {error}"));
	assert_eq!(
		targets,
		vec![crate::SubagentTarget::Claude, crate::SubagentTarget::Pi]
	);

	let invalid = String::from("nope");
	let invalid_error = crate::parse_subagent_targets(Some(vec![&invalid]))
		.err()
		.unwrap_or_else(|| panic!("expected invalid subagent target error"));
	assert!(
		invalid_error
			.to_string()
			.contains("unsupported subagent target `nope`")
	);

	let empty_values: Option<Vec<&String>> = None;
	let empty_error = crate::parse_subagent_targets(empty_values)
		.err()
		.unwrap_or_else(|| panic!("expected missing subagent targets error"));
	assert!(
		empty_error
			.to_string()
			.contains("expected at least one subagent target or `--all`")
	);
}

#[test]
fn subagents_command_supports_all_targets_in_default_text_output() {
	let _guard = snapshot_settings().bind_to_scope();
	let fixture = setup_scenario_workspace("subagents/basic");
	let output = run_cli(
		fixture.path(),
		[
			OsString::from("monochange"),
			OsString::from("subagents"),
			OsString::from("--all"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("subagents all dry-run text: {error}"));

	insta::assert_snapshot!("subagents_all_text_dry_run", output);
}

#[test]
fn cli_command_after_help_covers_supported_commands_and_custom_commands() {
	let cases = [
		("change", "Prefer configured package ids"),
		("release", "Direct package changes propagate to dependents"),
		("commit-release", "Embeds a durable release record block"),
		("affected", "Group-owned changesets cover all members"),
		("diagnostics", "linked review request"),
		("repair-release", "Defaults to descendant-only retargets"),
		(
			"step publish-release",
			"monochange step publish-release --prepared-release prepared-release.json",
		),
		(
			"step prepare-release",
			"monochange step prepare-release --output prepared-release.json",
		),
		(
			"step affected-packages",
			"Computes packages affected by a change or release plan",
		),
		(
			"step tag-release",
			"Treats reruns on the same commit as already up to date",
		),
	];

	for (name, expected) in cases {
		let after_help = cli_command_after_help(&CliCommandDefinition {
			name: name.to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
			dry_run: false,
		})
		.unwrap_or_else(|| panic!("expected after_help for {name}"));
		assert!(after_help.contains(expected));
	}

	assert!(
		cli_command_after_help(&CliCommandDefinition {
			name: "custom".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
			dry_run: false
		})
		.is_none()
	);
}

#[test]
fn render_tag_name_and_provider_urls_follow_provider_conventions() {
	let github = sample_github_source_configuration("https://api.github.com");
	assert_eq!(
		crate::render_tag_name("core", "1.2.3", "cargo", &VersionFormat::Primary),
		"v1.2.3"
	);
	assert_eq!(
		crate::render_tag_name("core", "1.2.3", "cargo", &VersionFormat::Namespaced),
		"core/v1.2.3"
	);
	assert_eq!(
		crate::render_tag_name(
			"core",
			"1.2.3",
			"cargo",
			&VersionFormat::Custom("{{ ecosystem }}/{{ name }}/v{{ version }}".to_string())
		),
		"cargo/core/v1.2.3"
	);
	assert!(crate::tag_url_for_provider(&github, "v1.2.3").contains("/releases/tag/v1.2.3"));
	assert!(
		crate::compare_url_for_provider(&github, "v1.2.2", "v1.2.3")
			.contains("compare/v1.2.2...v1.2.3")
	);

	let gitlab = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: Some("gitlab.example.com".to_string()),
		api_url: None,
		owner: "group".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	assert!(crate::tag_url_for_provider(&gitlab, "v1.2.3").contains("gitlab.example.com"));
	assert!(crate::compare_url_for_provider(&gitlab, "v1.2.2", "v1.2.3").contains("/-/compare/"));
}

#[test]
fn build_file_diff_previews_handles_missing_files_and_color_modes() {
	let tempdir = setup_scenario_workspace("cli-output/group-basic");
	let existing_path = tempdir.path().join("Cargo.toml");
	let missing_path = tempdir.path().join("new-file.md");
	let existing_content =
		fs::read(&existing_path).unwrap_or_else(|error| panic!("read existing file: {error}"));
	let mut updated_existing_content = existing_content.clone();
	updated_existing_content.extend_from_slice(b"\n# diff coverage\n");
	let updates = vec![
		crate::FileUpdate {
			path: existing_path.clone(),
			content: updated_existing_content.clone(),
		},
		crate::FileUpdate {
			path: missing_path.clone(),
			content: b"# created by coverage\n".to_vec(),
		},
	];

	let plain_previews = crate::build_file_diff_previews(tempdir.path(), &updates)
		.unwrap_or_else(|error| panic!("plain previews: {error}"));
	assert_eq!(plain_previews.len(), 2);
	assert!(
		plain_previews
			.iter()
			.all(|preview| preview.display_diff == preview.diff)
	);
	let rendered_plain_diffs = plain_previews
		.iter()
		.map(|preview| preview.diff.clone())
		.collect::<Vec<_>>()
		.join("\n---\n");
	assert!(rendered_plain_diffs.contains("new-file.md"));

	let colored_previews = crate::tests::block_on_in_context(
		crate::output::color::with_color_choice(crate::output::color::ColorChoice::Always, async {
			crate::build_file_diff_previews(
				tempdir.path(),
				&[crate::FileUpdate {
					path: existing_path.clone(),
					content: updated_existing_content,
				}],
			)
		}),
	)
	.unwrap_or_else(|error| panic!("colored previews: {error}"));
	assert_eq!(colored_previews.len(), 1);
	assert!(colored_previews[0].display_diff.contains("\u{1b}["));
	assert!(!colored_previews[0].diff.contains("\u{1b}["));
}

#[test]
fn render_discovery_report_supports_json_and_text_formats() {
	let report = monochange_core::DiscoveryReport {
		workspace_root: PathBuf::from("."),
		packages: vec![monochange_core::PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			PathBuf::from("crates/core/Cargo.toml"),
			PathBuf::from("."),
			Some(Version::new(1, 0, 0)),
			monochange_core::PublishState::Public,
		)],
		dependencies: Vec::new(),
		version_groups: vec![monochange_core::VersionGroup {
			group_id: "sdk".to_string(),
			display_name: "sdk".to_string(),
			members: vec!["cargo:crates/core/Cargo.toml".to_string()],
			member_max_bumps: std::collections::BTreeMap::new(),
			mismatch_detected: false,
		}],
		warnings: vec!["warning text".to_string()],
	};
	let json = crate::render_discovery_report(&report, crate::OutputFormat::Json)
		.unwrap_or_else(|error| panic!("json discovery: {error}"));
	assert!(json.contains("\"workspace_root\""));
	assert!(json.contains("\"warning text\""));

	let json_min = crate::render_discovery_report(&report, crate::OutputFormat::JsonMin)
		.unwrap_or_else(|error| panic!("json-min discovery: {error}"));
	let parsed_json: serde_json::Value =
		serde_json::from_str(&json).unwrap_or_else(|error| panic!("parse json discovery: {error}"));
	let parsed_json_min: serde_json::Value = serde_json::from_str(&json_min)
		.unwrap_or_else(|error| panic!("parse json-min discovery: {error}"));
	assert_eq!(parsed_json, parsed_json_min);
	assert!(!json_min.contains('\n'));
	assert!(json_min.contains("\"warning text\""));

	let text = crate::render_discovery_report(&report, crate::OutputFormat::Text)
		.unwrap_or_else(|error| panic!("text discovery: {error}"));
	assert!(
		text.lines()
			.last()
			.unwrap_or_default()
			.starts_with("✔ Discovered 1 package"),
		"{text}"
	);
	assert!(text.contains("Warnings (1)"), "{text}");
}

fn sample_planned_group() -> monochange_core::PlannedVersionGroup {
	monochange_core::PlannedVersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![
			"cargo:crates/core".to_string(),
			"cargo:crates/app".to_string(),
		],
		mismatch_detected: false,
		planned_version: Some(
			Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}")),
		),
		recommended_bump: BumpSeverity::Minor,
	}
}

fn sample_prepared_release_for_cli_render() -> crate::PreparedRelease {
	crate::PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![monochange_core::ReleaseDecision {
				package_id: "core".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}")),
				),
				group_id: Some("sdk".to_string()),
				reasons: vec!["release core".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}],
			groups: vec![sample_planned_group()],
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![crate::ReleaseTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: vec!["core".to_string(), "app".to_string()],
			rendered_title: "sdk 1.2.3".to_string(),
			rendered_changelog_title: "sdk changelog".to_string(),
			floating_tags: Vec::new(),
		}],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		package_publications: Vec::new(),
		dry_run: true,
		versioning: crate::versioning_state::ResolvedReleaseValues::default(),
	}
}

#[tokio::test(flavor = "multi_thread")]
async fn build_source_release_requests_and_change_request_cover_forgejo_dispatch() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Forgejo,
		host: Some("https://codeberg.org".to_string()),
		api_url: Some("https://codeberg.org/api/v1".to_string()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};
	assert_eq!(
		crate::hosted_sources::hosted_source_adapter(monochange_core::SourceProvider::Forgejo)
			.provider(),
		monochange_core::SourceProvider::Forgejo
	);
	assert_eq!(
		crate::tag_url_for_provider(&source, "v1.2.3"),
		"https://codeberg.org/ifiokjr/monochange/releases/tag/v1.2.3"
	);
	assert_eq!(
		crate::compare_url_for_provider(&source, "v1.2.2", "v1.2.3"),
		"https://codeberg.org/ifiokjr/monochange/compare/v1.2.2...v1.2.3"
	);
	let empty_publish = temp_env::async_with_vars([("FORGEJO_TOKEN", Some("token"))], async {
		crate::publish_source_release_requests(&source, &[])
			.await
			.unwrap_or_else(|error| panic!("publish empty Forgejo releases: {error}"))
	})
	.await;
	assert!(empty_publish.is_empty());

	let manifest = sample_release_manifest_for_commit_message(true, true);
	let requests = crate::build_source_release_requests(&source, &manifest);
	assert_eq!(requests.len(), 1);
	assert_eq!(
		requests[0].provider,
		monochange_core::SourceProvider::Forgejo
	);
	assert_eq!(requests[0].repository, "ifiokjr/monochange");
	let request = crate::build_source_change_request(&source, &manifest);
	assert_eq!(request.provider, monochange_core::SourceProvider::Forgejo);
	assert_eq!(request.repository, "ifiokjr/monochange");

	assert!(!request.commit_message.subject.is_empty());

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let publish_error = temp_env::async_with_vars([("FORGEJO_TOKEN", Some("invalid\n"))], async {
		crate::publish_source_change_request(&source, tempdir.path(), &request, &[], true, false)
			.await
			.err()
			.unwrap_or_else(|| panic!("expected invalid git repository error"))
	})
	.await;
	let error_string = publish_error.to_string();
	assert!(
		error_string.contains("failed to read current git branch")
			|| error_string.contains("prepare release pull request branch"),
		"expected error to contain git branch or pull request message, got: {error_string}"
	);
}

fn sample_github_source_configuration(api_url: &str) -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		host: None,
		api_url: Some(api_url.to_string()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	}
}

fn init_git_repo(root: &Path) {
	git_in_temp_repo(root, &["init", "-b", "main"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
}

/// The original `PATH` captured at process start, before any `temp_env` test
/// can temporarily replace it with a fixture-only path. This prevents race
/// conditions where a parallel test's PATH mutation causes `git` to be
/// unavailable to other tests that spawn git commands.
fn original_path() -> &'static str {
	static PATH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
	PATH.get_or_init(|| std::env::var("PATH").unwrap_or_default())
}

fn sanitized_git_command() -> std::process::Command {
	let mut command = std::process::Command::new("git");
	command.env("PATH", original_path());
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}

	command
}

fn git_in_dir(root: &Path, args: &[&str]) {
	let status = sanitized_git_command()
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_in_temp_repo(root: &Path, args: &[&str]) {
	let status = sanitized_git_command()
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_temp_repo(root: &Path, args: &[&str]) -> String {
	let output = sanitized_git_command()
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_help_subcommand_renders_detailed_command_help() {
	let output = run_with_args(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("help"),
			OsString::from("step"),
			OsString::from("validate"),
		],
	)
	.await
	.unwrap_or_else(|error| panic!("help step validate output: {error}"));

	assert!(output.contains("validate"));
	assert!(output.contains("Validate configuration, changesets, and version groups"));
	assert!(output.contains("Usage"));
}

#[test]
fn detect_remote_owner_repo_reads_origin_remote_from_a_live_git_repo() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args([
			"remote",
			"add",
			"origin",
			"git@github.com:ifiokjr/monochange.git",
		])
		.output()
		.unwrap_or_else(|error| panic!("git remote add: {error}"));

	let remote = crate::workspace_ops::detect_remote_owner_repo(tempdir.path())
		.unwrap_or_else(|| panic!("expected remote info"));
	assert_eq!(remote.owner, "ifiokjr");
	assert_eq!(remote.repo, "monochange");
}

#[test]
fn init_with_provider_writes_source_section_and_commit_release_command() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("init"),
			OsString::from("--provider"),
			OsString::from("github"),
		],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("wrote"), "expected wrote message");
	assert!(
		config.contains("[source]"),
		"expected [source] section in config"
	);
	assert!(
		config.contains("provider = \"github\""),
		"expected github provider"
	);
	assert!(
		config.contains("[source.releases]"),
		"expected [source.releases]"
	);
	assert!(
		config.contains("[source.pull_requests]"),
		"expected [source.pull_requests]"
	);
	assert!(
		!config
			.lines()
			.any(|line| line.trim_start().starts_with("[cli.")),
		"expected no generated CLI commands"
	);
	assert!(
		config.contains("monochange step *"),
		"expected starter config to document direct step commands"
	);
	load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("generated provider config should parse: {error}"));
}

#[test]
fn lint_cli_lists_and_explains_rules() {
	let root = fixture_path("config/lint-settings");
	let list_output = run_cli(
		&root,
		[
			OsString::from("monochange"),
			OsString::from("lint"),
			OsString::from("list"),
			OsString::from("--format=json"),
		],
	)
	.unwrap_or_else(|error| panic!("lint list: {error}"));
	assert!(list_output.contains("cargo/recommended"));
	assert!(list_output.contains("npm/recommended"));
	assert!(list_output.contains("dart/recommended"));
	assert!(list_output.contains("dart/required-package-fields"));
	assert!(list_output.contains("dart/sdk-constraint-present"));
	assert!(list_output.contains("dart/no-unexpected-dependency-overrides"));

	assert!(list_output.contains("dart/internal-path-dependency-policy"));
	assert!(list_output.contains("dart/flutter-package-metadata-consistent"));

	let explain_output = run_cli(
		&root,
		[
			OsString::from("monochange"),
			OsString::from("lint"),
			OsString::from("explain"),
			OsString::from("cargo/internal-dependency-workspace"),
		],
	)
	.unwrap_or_else(|error| panic!("lint explain: {error}"));
	assert!(explain_output.contains("Internal dependency workspace"));
}

#[test]
fn lint_cli_new_scaffolds_rule_files() {
	let tempdir = setup_scenario_workspace("test-support/scenario-workspace");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("lint"),
			OsString::from("new"),
			OsString::from("cargo/no-path-dependencies"),
		],
	)
	.unwrap_or_else(|error| panic!("lint new: {error}"));
	assert!(output.contains("no_path_dependencies.rs"));
	assert!(
		tempdir
			.path()
			.join("crates/monochange_cargo/src/lints/no_path_dependencies.rs")
			.exists()
	);
}

#[test]
fn check_command_supports_only_rule_filters() {
	let root = fixture_path("config/lint-settings");
	let output = run_cli(
		&root,
		[
			OsString::from("monochange"),
			OsString::from("check"),
			OsString::from("--only"),
			OsString::from("cargo/internal-dependency-workspace"),
			OsString::from("--format=json"),
		],
	)
	.unwrap_or_else(|error| panic!("check only: {error}"));
	assert!(output.contains("\"results\""));
	assert!(output.contains("\"warning_count\""));
}

#[test]
fn quiet_lint_commands_return_empty_output() {
	let root = fixture_path("config/lint-settings");
	let output = run_cli(
		&root,
		[
			OsString::from("monochange"),
			OsString::from("--quiet"),
			OsString::from("lint"),
			OsString::from("list"),
		],
	)
	.unwrap_or_else(|error| panic!("quiet lint list: {error}"));
	assert!(output.is_empty());
}

#[test]
fn build_release_manifest_from_record_populates_manifest_from_release_record() {
	use std::path::PathBuf;

	use monochange_core::ChangelogFormat;
	use monochange_core::ReleaseManifestChangelog;
	use monochange_core::ReleaseNotesDocument;
	use monochange_core::ReleaseOwnerKind;
	use monochange_core::ReleaseRecord;
	use monochange_core::ReleaseRecordProvider;
	use monochange_core::ReleaseRecordTarget;

	let record = ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.0.0".to_string()),
		versions: BTreeMap::new(),
		release_targets: vec![ReleaseRecordTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.0.0".to_string(),
			version_format: VersionFormat::Namespaced,
			tag: true,
			release: true,
			tag_name: "core/v1.0.0".to_string(),
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
			members: vec!["core".to_string()],
			floating_tags: Vec::new(),
		}],
		released_packages: vec!["workflow-core".to_string()],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		package_publications: vec![],
		updated_changelogs: vec![PathBuf::from("crates/core/CHANGELOG.md")],
		deleted_changesets: vec![],
		changesets: vec![],
		changelogs: Vec::new(),
		provider: Some(ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	};

	let manifest = crate::release_artifacts::build_release_manifest_from_record(&record);
	assert_eq!(manifest.command, "release-pr");
	assert_eq!(manifest.version, Some("1.0.0".to_string()));
	assert_eq!(manifest.release_targets.len(), 1);
	assert_eq!(manifest.release_targets[0].id, "core");
	assert_eq!(manifest.changelogs.len(), 1);
	assert_eq!(
		manifest.changelogs[0],
		ReleaseManifestChangelog {
			path: PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
			owner_id: String::new(),
			owner_kind: ReleaseOwnerKind::Group,
			output: "default".to_string(),
			stream: "default".to_string(),
			notes: ReleaseNotesDocument {
				title: String::new(),
				summary: Vec::new(),
				sections: Vec::new(),
			},
			rendered: String::new(),
		}
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_publish_release_falls_back_to_release_record_from_git() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"[source]
provider = "github"
owner = "monochange"
repo = "monochange"

[source.releases]
branches = ["release/*"]
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.email", "test@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "commit.gpgsign", "false"])
		.output()
		.unwrap_or_else(|error| panic!("git config gpgsign: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.name", "Test User"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "commit.gpgsign", "false"])
		.output()
		.unwrap_or_else(|error| panic!("git config signing: {error}"));
	fs::write(tempdir.path().join("tracked.txt"), "test\n")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "tracked.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "initial commit"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));

	// Write a release record block into a file and commit it
	let record = monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.0.0".to_string()),
		versions: BTreeMap::new(),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "test".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.0.0".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.0.0".to_string(),
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
			members: vec![],
			floating_tags: Vec::new(),
		}],
		released_packages: vec![],
		changed_files: vec![],
		package_publications: vec![],
		updated_changelogs: vec![],
		deleted_changesets: vec![],
		changesets: vec![monochange_core::PreparedChangeset {
			path: PathBuf::from(".changeset/test.md"),
			summary: Some("Test changeset".to_string()),
			details: None,
			targets: vec![],
			context: Some(monochange_core::ChangesetContext {
				provider: monochange_core::HostingProviderKind::GitHub,
				host: None,
				capabilities: monochange_core::HostingCapabilities::default(),
				introduced: None,
				last_updated: None,
				related_issues: vec![monochange_core::HostedIssueRef {
					provider: monochange_core::HostingProviderKind::GitHub,
					host: None,
					id: "#1".to_string(),
					title: None,
					url: None,
					relationship:
						monochange_core::HostedIssueRelationshipKind::ClosedByReviewRequest,
				}],
			}),
		}],
		changelogs: Vec::new(),
		provider: None,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	};
	let json = serde_json::to_string_pretty(&record)
		.unwrap_or_else(|error| panic!("serialize release record: {error}"));
	let hash = {
		use std::collections::hash_map::DefaultHasher;
		use std::hash::Hasher;
		let mut hasher = DefaultHasher::new();
		for target in &record.release_targets {
			hasher.write(target.id.as_bytes());
			hasher.write(target.version.as_bytes());
		}

		format!("{:016x}", hasher.finish())
	};
	let dir = tempdir.path().join(".monochange/releases").join(&hash);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create release record dir: {error}"));
	let record_path = dir.join("release.json");
	fs::write(&record_path, &json).unwrap_or_else(|error| panic!("write release record: {error}"));
	fs::write(tempdir.path().join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "."])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "chore(release): prepare release"])
		.output()
		.unwrap_or_else(|error| panic!("git commit release: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::PublishRelease {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};
	let result = execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	);
	// It will fail on release branch policy, but it should get past the fallback.
	let error = result
		.await
		.err()
		.unwrap_or_else(|| panic!("expected error"));
	let message = error.to_string();
	assert!(
		message.contains("configured release branch pattern [release/*]"),
		"unexpected error: {message}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn execute_cli_command_comment_released_issues_falls_back_to_release_record_from_git() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.email", "test@example.com"])
		.output()
		.unwrap_or_else(|error| panic!("git config email: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "commit.gpgsign", "false"])
		.output()
		.unwrap_or_else(|error| panic!("git config gpgsign: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["config", "user.name", "Test User"])
		.output()
		.unwrap_or_else(|error| panic!("git config name: {error}"));
	fs::write(tempdir.path().join("tracked.txt"), "test\n")
		.unwrap_or_else(|error| panic!("write tracked: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "tracked.txt"])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));

	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "initial commit"])
		.output()
		.unwrap_or_else(|error| panic!("git commit: {error}"));

	let record = monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: "monochange.releaseRecord".to_string(),
		created_at: "2024-01-01T00:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.0.0".to_string()),
		versions: BTreeMap::new(),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "test".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.0.0".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.0.0".to_string(),
			rendered_title: String::new(),
			rendered_changelog_title: String::new(),
			members: vec![],
			floating_tags: Vec::new(),
		}],
		released_packages: vec![],
		changed_files: vec![],
		package_publications: vec![],
		updated_changelogs: vec![],
		deleted_changesets: vec![],
		changesets: vec![monochange_core::PreparedChangeset {
			path: PathBuf::from(".changeset/test.md"),
			summary: Some("Test changeset".to_string()),
			details: None,
			targets: vec![],
			context: Some(monochange_core::ChangesetContext {
				provider: monochange_core::HostingProviderKind::GitHub,
				host: None,
				capabilities: monochange_core::HostingCapabilities::default(),
				introduced: None,
				last_updated: None,
				related_issues: vec![monochange_core::HostedIssueRef {
					provider: monochange_core::HostingProviderKind::GitHub,
					host: None,
					id: "#1".to_string(),
					title: None,
					url: None,
					relationship:
						monochange_core::HostedIssueRelationshipKind::ClosedByReviewRequest,
				}],
			}),
		}],
		changelogs: Vec::new(),
		provider: None,
		label_inputs: monochange_core::versioning::LabelInputs::default(),
		labels: BTreeMap::new(),
		values: BTreeMap::new(),
	};
	let json = serde_json::to_string_pretty(&record)
		.unwrap_or_else(|error| panic!("serialize release record: {error}"));
	let hash = {
		use std::collections::hash_map::DefaultHasher;
		use std::hash::Hasher;
		let mut hasher = DefaultHasher::new();
		for target in &record.release_targets {
			hasher.write(target.id.as_bytes());
			hasher.write(target.version.as_bytes());
		}

		format!("{:016x}", hasher.finish())
	};
	let dir = tempdir.path().join(".monochange/releases").join(&hash);
	fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("create release record dir: {error}"));
	let record_path = dir.join("release.json");
	fs::write(&record_path, &json).unwrap_or_else(|error| panic!("write release record: {error}"));
	fs::write(tempdir.path().join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["add", "."])
		.output()
		.unwrap_or_else(|error| panic!("git add: {error}"));
	std::process::Command::new("git")
		.current_dir(tempdir.path())
		.args(["commit", "-m", "chore(release): prepare release"])
		.output()
		.unwrap_or_else(|error| panic!("git commit release: {error}"));

	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = CliCommandDefinition {
		name: "release-comments".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::CommentReleasedIssues {
			name: None,
			when: None,
			always_run: false,
			inputs: BTreeMap::new(),
		}],
		dry_run: false,
	};
	let result = execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		true,
		BTreeMap::new(),
	);
	result
		.await
		.unwrap_or_else(|error| panic!("unexpected error: {error}"));
}

#[tokio::test(flavor = "multi_thread")]
async fn version_and_root_help_skip_workspace_validation() {
	let help_command = [
		OsString::from("monochange"),
		OsString::from("help"),
		OsString::from("--help"),
	];
	assert_eq!(crate::command_help_request(&help_command), None);

	let tempdir = monochange_test_helpers::setup_fixture!("monochange/help-invalid-workspace");

	let version = run_with_args_in_dir(
		"monochange",
		[OsString::from("monochange"), OsString::from("--version")],
		tempdir.path(),
	)
	.await
	.unwrap_or_else(|error| panic!("version output: {error}"));
	assert!(version.contains("monochange "));

	let help = run_with_args_in_dir(
		"monochange",
		[OsString::from("monochange"), OsString::from("--help")],
		tempdir.path(),
	)
	.await
	.unwrap_or_else(|error| panic!("help output: {error}"));
	insta::assert_snapshot!("invalid_workspace_root_help", help);

	let traced_help = run_with_args_in_dir(
		"monochange",
		[
			OsString::from("monochange"),
			OsString::from("--log-level"),
			OsString::from("trace"),
			OsString::from("--help"),
		],
		tempdir.path(),
	)
	.await
	.unwrap_or_else(|error| panic!("traced help output: {error}"));
	assert_eq!(traced_help, help);
}

#[test]
fn cli_snapshot_classification_reports_parse_and_missing_value_errors() {
	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--cli-snapshot-before"),
	];
	let error = render_cli_snapshot_classification(&args).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("missing value for --cli-snapshot-before")
	);

	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--cli-snapshot-after"),
	];
	let error = render_cli_snapshot_classification(&args).unwrap_err();
	assert!(
		error
			.to_string()
			.contains("missing value for --cli-snapshot-after")
	);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let invalid = tempdir.path().join("invalid.json");
	fs::write(&invalid, "not json").unwrap_or_else(|error| panic!("write invalid: {error}"));
	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--cli-snapshot-before"),
		invalid.clone().into_os_string(),
		OsString::from("--cli-snapshot-after"),
		OsString::from("missing.json"),
	];
	let error = render_cli_snapshot_classification(&args).unwrap_err();
	assert!(error.to_string().contains("failed to parse CLI snapshot"));

	let missing = tempdir.path().join("missing.json");
	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--cli-snapshot-before"),
		missing.into_os_string(),
		OsString::from("--cli-snapshot-after"),
		invalid.into_os_string(),
	];
	let error = render_cli_snapshot_classification(&args).unwrap_err();
	assert!(error.to_string().contains("failed to read CLI snapshot"));

	let args = vec![OsString::from("monochange")];
	assert!(render_cli_snapshot_classification(&args).unwrap().is_none());

	let args = vec![OsString::from("monochange"), OsString::from("step")];
	assert!(render_cli_snapshot_classification(&args).unwrap().is_none());

	let args = vec![
		OsString::from("monochange"),
		OsString::from("step"),
		OsString::from("classify"),
	];
	assert!(render_cli_snapshot_classification(&args).unwrap().is_none());

	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--ignored"),
	];
	assert!(render_cli_snapshot_classification(&args).unwrap().is_none());
}

#[test]
fn cli_snapshot_classification_renders_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let before_path = tempdir.path().join("before.json");
	let after_path = tempdir.path().join("after.json");
	let before = monochange_snapshot::snapshot_from_clap(&Command::new("demo"));
	let after = monochange_snapshot::snapshot_from_clap(
		&Command::new("demo").subcommand(Command::new("run")),
	);
	fs::write(
		&before_path,
		before
			.to_json()
			.unwrap_or_else(|error| panic!("before json: {error}")),
	)
	.unwrap_or_else(|error| panic!("write before: {error}"));
	fs::write(
		&after_path,
		after
			.to_json()
			.unwrap_or_else(|error| panic!("after json: {error}")),
	)
	.unwrap_or_else(|error| panic!("write after: {error}"));

	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--format"),
		OsString::from("json"),
		OsString::from("--cli-snapshot-before"),
		before_path.into_os_string(),
		OsString::from("--cli-snapshot-after"),
		after_path.into_os_string(),
	];
	let output = render_cli_snapshot_classification(&args)
		.unwrap_or_else(|error| panic!("classification: {error}"))
		.unwrap_or_else(|| panic!("classification output"));

	assert!(output.contains("\"recommendation\": \"minor\""));

	let before_path = tempdir.path().join("before-text.json");
	let after_path = tempdir.path().join("after-text.json");
	let before = monochange_snapshot::snapshot_from_clap(&Command::new("demo"));
	let after = monochange_snapshot::snapshot_from_clap(
		&Command::new("demo").subcommand(Command::new("deploy")),
	);
	fs::write(
		&before_path,
		before
			.to_json()
			.unwrap_or_else(|error| panic!("before text json: {error}")),
	)
	.unwrap_or_else(|error| panic!("write before text: {error}"));
	fs::write(
		&after_path,
		after
			.to_json()
			.unwrap_or_else(|error| panic!("after text json: {error}")),
	)
	.unwrap_or_else(|error| panic!("write after text: {error}"));
	let args = vec![
		OsString::from("monochange"),
		OsString::from("change"),
		OsString::from("classify"),
		OsString::from("--cli-snapshot-before"),
		before_path.into_os_string(),
		OsString::from("--cli-snapshot-after"),
		after_path.into_os_string(),
	];
	let output = render_cli_snapshot_classification(&args)
		.unwrap_or_else(|error| panic!("text classification: {error}"))
		.unwrap_or_else(|| panic!("text classification output"));

	assert!(output.contains("cli snapshot recommendation: minor"));
	assert!(output.contains("command `deploy` was added"));
}

#[tokio::test(flavor = "multi_thread")]
async fn config_commands_run_at_the_top_level_and_report_command_steps_as_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.announce]
help_text = "Announce a release"

[[cli.announce.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.announce.steps]]
name = "say hello"
type = "Command"
id = "hello"
command = "printf hello"
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	let run = |args: &[&str]| {
		let mut arguments = vec![OsString::from("monochange")];
		arguments.extend(args.iter().map(OsString::from));
		run_with_args_in_dir("monochange", arguments, tempdir.path())
	};

	let output = run(&["announce", "--format", "json"])
		.await
		.unwrap_or_else(|error| panic!("announce json: {error}"));
	let value = serde_json::from_str::<serde_json::Value>(&output)
		.unwrap_or_else(|error| panic!("parse announce json: {error}\n{output}"));
	assert_eq!(value["command"], "announce");
	assert_eq!(value["dry_run"], false);
	assert_eq!(value["commands"][0]["step"], "say hello");
	assert_eq!(value["commands"][0]["id"], "hello");
	assert_eq!(value["commands"][0]["status"], "succeeded");
	assert_eq!(value["commands"][0]["exit_code"], 0);

	assert_eq!(value["commands"][0]["stdout"], "hello");

	let dry_run = run(&["announce", "--dry-run", "--format", "json"])
		.await
		.unwrap_or_else(|error| panic!("announce dry-run json: {error}"));
	let value = serde_json::from_str::<serde_json::Value>(&dry_run)
		.unwrap_or_else(|error| panic!("parse dry-run json: {error}\n{dry_run}"));
	assert_eq!(value["dry_run"], true);
	assert_eq!(value["commands"][0]["status"], "skipped");
	assert_eq!(value["commands"][0]["command"], "printf hello");
	assert_eq!(value["commands"][0]["exit_code"], serde_json::Value::Null);

	let text = run(&["announce"])
		.await
		.unwrap_or_else(|error| panic!("announce text: {error}"));
	assert!(text.contains("hello"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn cli_surface_snapshots_list_workflows_only_under_run() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.announce]
help_text = "Announce a release"
steps = [{ name = "say hello", type = "Command", command = "printf hello" }]
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	let run = |args: &[&str]| {
		let mut arguments = vec![OsString::from("monochange")];
		arguments.extend(args.iter().map(OsString::from));
		run_with_args_in_dir("monochange", arguments, tempdir.path())
	};
	let top_level_paths = |output: &str| {
		serde_json::from_str::<serde_json::Value>(output)
			.unwrap_or_else(|error| panic!("parse snapshot: {error}\n{output}"))["commands"]
			.as_array()
			.unwrap_or_else(|| panic!("snapshot commands"))
			.iter()
			.map(|command| command["path"].to_string())
			.collect::<Vec<_>>()
	};

	for args in [&["snapshot", "--view", "index"][..], &["--snapshot"][..]] {
		let paths = top_level_paths(
			&run(args)
				.await
				.unwrap_or_else(|error| panic!("{args:?}: {error}")),
		);
		assert!(!paths.contains(&r#"["announce"]"#.to_string()), "{paths:?}");
		assert!(paths.contains(&r#"["run"]"#.to_string()), "{paths:?}");
	}

	let run_subtree = run(&["snapshot", "run", "announce"])
		.await
		.unwrap_or_else(|error| panic!("run announce snapshot: {error}"));
	assert!(run_subtree.contains("Announce a release"), "{run_subtree}");

	let workflow_subtree = run(&["announce", "--snapshot"])
		.await
		.unwrap_or_else(|error| panic!("announce snapshot: {error}"));
	assert_eq!(top_level_paths(&workflow_subtree), [r#"["announce"]"#]);
}

#[tokio::test(flavor = "multi_thread")]
async fn command_groups_without_a_subcommand_list_their_subcommands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = run_with_args_in_dir(
		"monochange",
		[OsString::from("monochange"), OsString::from("changeset")],
		tempdir.path(),
	)
	.await
	.expect_err("a command group without a subcommand is a usage error")
	.to_string();

	assert!(
		error.starts_with("error: `monochange changeset` needs a subcommand"),
		"{error}"
	);
	assert!(error.contains("validate"), "{error}");
	assert!(!error.contains("Options:"), "{error}");
	assert!(
		error.ends_with("For more information, try 'monochange changeset --help'."),
		"{error}"
	);
}

#[test]
fn validate_counts_warnings_in_its_summary() {
	let tempdir = setup_fixture("config/versioned-file-empty-glob");
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("monochange"),
			OsString::from("step"),
			OsString::from("validate"),
		],
	)
	.unwrap_or_else(|error| panic!("validate with warnings: {error}"));

	assert_eq!(output, "✔ Workspace validation passed · 1 warning");
}

#[tokio::test(flavor = "multi_thread")]
async fn completions_and_doctor_dispatch_render_outputs() {
	let workspace = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(
		workspace.path().join("monochange.toml"),
		"[ecosystems.deno]\nenabled = false\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));

	let completions = run_cli(
		workspace.path(),
		[
			OsString::from("monochange"),
			OsString::from("completions"),
			OsString::from("bash"),
		],
	)
	.unwrap_or_else(|error| panic!("completions dispatch: {error}"));
	assert!(
		completions.contains("_monochange()"),
		"completions dispatch: {}",
		&completions[..completions.len().min(200)]
	);

	let doctor = run_cli(
		workspace.path(),
		[OsString::from("monochange"), OsString::from("doctor")],
	)
	.unwrap_or_else(|error| panic!("doctor dispatch: {error}"));
	assert!(
		doctor.contains("Environment checks"),
		"doctor dispatch: {doctor}"
	);
}
