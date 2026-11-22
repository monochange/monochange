#![allow(clippy::disallowed_methods)]
use std::fs;
use std::path::PathBuf;

use super::*;
use crate::cli::build_lint_subcommand;

fn clean_lint_workspace() -> tempfile::TempDir {
	monochange_test_helpers::setup_scenario_workspace!("lint-check/clean")
}

fn readonly_fix_workspace() -> tempfile::TempDir {
	monochange_test_helpers::setup_scenario_workspace!("lint-check/read-only-fix")
}

fn type_scoped_changeset_workspace() -> tempfile::TempDir {
	monochange_test_helpers::setup_scenario_workspace!("lint-check/type-scope")
}

fn run_check_command(
	root: &std::path::Path,
	fix: bool,
	fix_dry_run: bool,
	ecosystems: &[String],
	only_rules: &[String],
	format: OutputFormat,
	verbose: bool,
) -> MonochangeResult<String> {
	let configuration = monochange_config::load_workspace_configuration(root)?;
	let reporter = crate::output::ProgressReporter::named(
		"check",
		false,
		true,
		crate::output::ProgressFormat::Auto,
	);
	run_check_command_with_configuration(
		root,
		&configuration,
		fix,
		fix_dry_run,
		ecosystems,
		only_rules,
		format,
		verbose,
		&reporter,
	)
}

fn check_report_input<'a>(
	report: &'a LintReport,
	validation_warnings: &'a [String],
	validation_errors: &'a [String],
	fixed_files: usize,
	fix_dry_run: bool,
	verbose: bool,
) -> CheckReportInput<'a> {
	CheckReportInput {
		root: Path::new("/workspace"),
		report,
		validation_warnings,
		validation_errors,
		fixed_files,
		fix_dry_run,
		verbose,
	}
}

#[test]
fn check_report_explains_validation_warnings_fixes_and_remaining_fixable_issues() {
	let mut report = LintReport::new();
	report.add(
		monochange_core::lint::LintResult::new(
			"npm/sorted-dependencies",
			monochange_core::lint::LintLocation::new("/workspace/packages/app/package.json", 12, 4)
				.with_span(3, 9),
			"dependencies are not sorted",
			LintSeverity::Warning,
		)
		.with_fix(monochange_core::lint::LintFix::single(
			"sort dependencies",
			(3, 9),
			"{}",
		)),
	);
	report.add(monochange_core::lint::LintResult::new(
		"npm/off-rule",
		monochange_core::lint::LintLocation::new("/workspace/packages/app/package.json", 2, 1),
		"informational",
		LintSeverity::Off,
	));
	report
		.warnings
		.push("skipped autofix for Cargo.toml".to_string());

	let warnings_only = format_check_report(
		&check_report_input(
			&report,
			&["deprecated option".to_string()],
			&[],
			0,
			false,
			false,
		),
		TextTheme::for_stdout(),
	);
	insta::assert_snapshot!("check_report_warnings_only", warnings_only);

	let failed_after_fix = format_check_report(
		&check_report_input(
			&report,
			&[],
			&["error: package `x` path `x` does not exist\n  --> monochange.toml:3:1".to_string()],
			2,
			false,
			true,
		),
		TextTheme::for_stdout(),
	);
	insta::assert_snapshot!(
		"check_report_validation_failure_after_fix",
		failed_after_fix
	);
}

#[test]
fn run_check_command_supports_text_json_and_fix_error_paths() {
	let clean_workspace = clean_lint_workspace();
	let json = run_check_command(
		clean_workspace.path(),
		false,
		false,
		&[],
		&[],
		OutputFormat::Json,
		false,
	)
	.unwrap();
	assert!(json.contains("\"error_count\": 0"));

	let text = run_check_command(
		clean_workspace.path(),
		false,
		false,
		&[],
		&[],
		OutputFormat::Text,
		false,
	)
	.unwrap();
	assert_eq!(text, "✔ Checks passed");

	let tempdir = readonly_fix_workspace();
	let cargo_toml = tempdir.path().join("crates/example/Cargo.toml");
	let mut permissions = fs::metadata(&cargo_toml).unwrap().permissions();
	permissions.set_readonly(true);
	fs::set_permissions(&cargo_toml, permissions).unwrap();
	let error = run_check_command(
		tempdir.path(),
		true,
		false,
		&[],
		&[],
		OutputFormat::Text,
		false,
	)
	.expect_err("expected fix write to fail for readonly manifest");
	assert!(error.to_string().contains("Failed to write fixed content"));
}

#[test]
fn run_check_command_enforces_type_scoped_changeset_rules() {
	let workspace = type_scoped_changeset_workspace();
	let error = run_check_command(
		workspace.path(),
		false,
		false,
		&[],
		&[],
		OutputFormat::Text,
		false,
	)
	.expect_err("expected the missing developer notes section to fail");
	let message = error
		.reported_output()
		.expect("text check failures preserve the complete result");

	assert!(message.contains("changesets/types/app_feature"));
	assert!(message.contains("changeset must include a `Developer notes` section"));

	let error = run_check_command(
		workspace.path(),
		false,
		false,
		&[],
		&[],
		OutputFormat::Json,
		false,
	)
	.expect_err("expected the JSON check to preserve the failure exit status");
	assert!(
		error
			.to_string()
			.contains("check failed: 1 error, 0 warnings")
	);
}

#[test]
fn run_check_command_reports_all_validation_errors_before_failing() {
	let tempdir = tempfile::tempdir()
		.unwrap_or_else(|error| panic!("expected tempdir to be created: {error}"));
	let root = tempdir.path();
	let crate_dir = root.join("crates/core");
	fs::create_dir_all(&crate_dir)
		.unwrap_or_else(|error| panic!("expected crate dir to be created: {error}"));
	fs::create_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("expected changeset dir to be created: {error}"));
	fs::write(
		root.join("monochange.toml"),
		"[package.core]\n\
		path = \"crates/core\"\n\
		type = \"cargo\"\n\
		versioned_files = [\n\
		  { path = \"missing.toml\", type = \"cargo\" },\n\
		  { path = \"missing/*.toml\", type = \"cargo\" },\n\
		]\n",
	)
	.unwrap_or_else(|error| panic!("expected config to be written: {error}"));
	fs::write(
		crate_dir.join("Cargo.toml"),
		"[package]\n\
		name = \"core\"\n\
		version = \"1.0.0\"\n\
		edition = \"2021\"\n\
		description = \"Test core package\"\n\
		license = \"MIT\"\n\
		repository = \"https://github.com/monochange/monochange\"\n",
	)
	.unwrap_or_else(|error| panic!("expected manifest to be written: {error}"));
	fs::write(
		root.join(".changeset/broken.md"),
		"---\nmissing: patch\n---\n\n# Broken target\n",
	)
	.unwrap_or_else(|error| panic!("expected changeset to be written: {error}"));

	let (warnings, errors) = collect_workspace_validation_issues(
		root,
		&monochange_config::load_workspace_configuration(root).unwrap(),
	);
	assert!(warnings.is_empty());
	assert!(
		errors
			.iter()
			.any(|error| error.contains("unknown package or group `missing`"))
	);
	assert!(
		errors
			.iter()
			.any(|error| error.contains("versioned file") && error.contains("missing.toml"))
	);

	let warning_tempdir = tempfile::tempdir()
		.unwrap_or_else(|error| panic!("expected warning tempdir to be created: {error}"));
	let warning_root = warning_tempdir.path();
	let warning_crate_dir = warning_root.join("crates/core");
	fs::create_dir_all(&warning_crate_dir)
		.unwrap_or_else(|error| panic!("expected warning crate dir to be created: {error}"));
	fs::write(
		warning_root.join("monochange.toml"),
		"[package.core]\n\
		path = \"crates/core\"\n\
		type = \"cargo\"\n\
		versioned_files = [{ path = \"missing/*.toml\", type = \"cargo\" }]\n",
	)
	.unwrap_or_else(|error| panic!("expected warning config to be written: {error}"));
	fs::write(
		warning_crate_dir.join("Cargo.toml"),
		"[package]\n\
		name = \"core\"\n\
		version = \"1.0.0\"\n\
		edition = \"2021\"\n\
		description = \"Test core package\"\n\
		license = \"MIT\"\n\
		repository = \"https://github.com/monochange/monochange\"\n",
	)
	.unwrap_or_else(|error| panic!("expected warning manifest to be written: {error}"));
	let warning_text = run_check_command(
		warning_root,
		false,
		false,
		&[],
		&[],
		OutputFormat::Text,
		false,
	)
	.unwrap_or_else(|error| panic!("expected warning-only check to pass: {error}"));
	assert!(
		warning_text
			.lines()
			.last()
			.unwrap_or_default()
			.starts_with("▲ Checks passed with warnings"),
		"{warning_text}"
	);

	assert!(warning_text.contains("matches no files"));

	let text_error = run_check_command(root, false, false, &[], &[], OutputFormat::Text, false)
		.expect_err("expected text check to fail validation");
	// The printed report lists every validation error; the diagnostic only
	// summarizes the failure.
	assert!(text_error.to_string().starts_with("check failed:"));
	let text_report = text_error
		.reported_output()
		.unwrap_or_else(|| panic!("expected a reported check result"));
	assert!(
		text_report.contains("Workspace validation (2)"),
		"{text_report}"
	);
	assert!(text_report.contains("unknown package or group `missing`"));
	assert!(text_report.contains("missing.toml"));

	let json_error = run_check_command(root, false, false, &[], &[], OutputFormat::Json, false)
		.expect_err("expected json check to fail validation");
	let json_message = json_error.to_string();
	assert!(json_message.contains("check failed"));
	assert!(json_message.contains("workspace validation failed"));
}

#[test]
fn run_lint_step_supports_fix_write_failures() {
	let tempdir = readonly_fix_workspace();
	let cargo_toml = tempdir.path().join("crates/example/Cargo.toml");
	let mut permissions = fs::metadata(&cargo_toml).unwrap().permissions();
	permissions.set_readonly(true);
	fs::set_permissions(&cargo_toml, permissions).unwrap();
	let error =
		run_lint_step(tempdir.path(), true).expect_err("expected lint-step fix write failure");
	assert!(error.to_string().contains("Failed to write fixed content"));
}

#[test]
fn scaffold_lint_rule_reports_directory_and_file_conflicts() {
	let tempdir = tempfile::tempdir().unwrap();
	let lint_dir = tempdir.path().join("crates/monochange_cargo/src/lints");
	fs::create_dir_all(lint_dir.parent().unwrap()).unwrap();
	fs::write(&lint_dir, "not a directory").unwrap();
	let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
		.expect_err("expected lint-dir creation to fail");
	assert!(
		error
			.to_string()
			.contains("failed to create lint directory")
	);

	let tempdir = tempfile::tempdir().unwrap();
	fs::create_dir_all(tempdir.path().join("crates/monochange_cargo/src/lints")).unwrap();
	fs::create_dir_all(
		tempdir
			.path()
			.join("fixtures/tests/lints/cargo/no-path-dependencies/workspace"),
	)
	.unwrap();
	fs::write(
		tempdir
			.path()
			.join("crates/monochange_cargo/src/lints/no_path_dependencies.rs"),
		"existing",
	)
	.unwrap();
	let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
		.expect_err("expected existing lint file to fail");
	assert!(error.to_string().contains("already exists"));

	let tempdir = tempfile::tempdir().unwrap();
	fs::create_dir_all(tempdir.path().join("crates/monochange_cargo/src/lints")).unwrap();
	fs::create_dir_all(
		tempdir
			.path()
			.join("fixtures/tests/lints/cargo/no-path-dependencies"),
	)
	.unwrap();
	fs::write(
		tempdir
			.path()
			.join("fixtures/tests/lints/cargo/no-path-dependencies/workspace"),
		"not a directory",
	)
	.unwrap();
	let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
		.expect_err("expected fixture-dir creation to fail");
	assert!(
		error
			.to_string()
			.contains("failed to create lint fixture directory")
	);
}

#[test]
fn lint_explanations_lead_with_the_id_and_align_their_details() {
	let rule_with_options = available_lint_rules()
		.into_iter()
		.find(|rule| !rule.options.is_empty())
		.unwrap_or_else(|| panic!("expected a lint rule with options"));
	let rule = render_lint_explanation(&rule_with_options.id, OutputFormat::Text).unwrap();
	assert!(
		rule.starts_with(&format!("• {} · ", rule_with_options.id)),
		"{rule}"
	);
	assert!(rule.contains("\nCategory  "), "{rule}");
	assert!(rule.contains("\nOptions ("), "{rule}");

	let preset = render_lint_explanation("cargo/recommended", OutputFormat::Text).unwrap();
	assert!(preset.starts_with("• cargo/recommended · "), "{preset}");
	assert!(preset.contains("\nMaturity  "), "{preset}");
	assert!(preset.contains("\nRules ("), "{preset}");
}
