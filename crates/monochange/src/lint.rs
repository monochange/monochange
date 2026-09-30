//! Check command implementation for monochange CLI.
//!
//! `monochange check` combines workspace validation with manifest lint enforcement.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;

use clap::ArgMatches;
use monochange_config::load_workspace_configuration;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintProgressReporter;
use monochange_core::lint::LintReport;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_lint::LintSelection;
use monochange_lint::Linter;

use crate::OutputFormat;
use crate::output::text::Outcome;
use crate::output::text::TableCell;
use crate::output::text::TextReport;
use crate::output::text::TextTheme;
use crate::output::text::Tone;
use crate::output::text::plural;
use crate::root_relative;

#[allow(clippy::vec_init_then_push)]
fn lint_suites(configuration: Option<&WorkspaceConfiguration>) -> Vec<Box<dyn LintSuite>> {
	let mut suites: Vec<Box<dyn LintSuite>> = Vec::new();
	#[cfg(feature = "cargo")]
	suites.push(Box::new(monochange_cargo::lints::lint_suite()));
	#[cfg(feature = "npm")]
	suites.push(Box::new(monochange_npm::lints::lint_suite()));
	#[cfg(feature = "dart")]
	suites.push(Box::new(monochange_dart::lints::lint_suite()));
	let changeset_suite =
		configuration.map_or_else(monochange_config::lints::lint_suite, |config| {
			monochange_config::lints::ChangesetLintSuite::with_change_types(
				config.changelog.types.keys().cloned(),
			)
		});
	suites.push(Box::new(changeset_suite));
	suites
}

fn build_linter(configuration: &WorkspaceConfiguration, selection: LintSelection) -> Linter {
	Linter::new(
		lint_suites(Some(configuration)),
		configuration.lints.clone(),
	)
	.with_selection(selection)
}

pub(crate) fn collect_workspace_validation_issues(
	root: &Path,
	configuration: &WorkspaceConfiguration,
) -> (Vec<String>, Vec<String>) {
	let mut warnings = Vec::new();
	let mut errors = Vec::new();

	if let Err(error) = monochange_config::validate_workspace_with_config(root, configuration) {
		errors.push(error.render());
	}

	let ecosystems = crate::workspace_ops::build_ecosystem_registry();
	match monochange_config::validate_versioned_files_content_with_config(
		root,
		configuration,
		&ecosystems,
	) {
		Ok(mut collected_warnings) => warnings.append(&mut collected_warnings),
		Err(error) => errors.push(error.render()),
	}

	#[cfg(feature = "cargo")]
	if let Err(error) = crate::workspace_ops::validate_cargo_workspace_version_groups(root) {
		errors.push(error.render());
	}

	(warnings, errors)
}

pub(crate) fn available_lint_rules() -> Vec<LintRule> {
	let mut rules = Linter::new(
		lint_suites(None),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.rules();
	rules.sort_by(|left, right| left.id.cmp(&right.id));
	rules
}

pub(crate) fn available_lint_presets() -> Vec<LintPreset> {
	let mut presets = Linter::new(
		lint_suites(None),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.presets();
	presets.sort_by(|left, right| left.id.cmp(&right.id));
	presets
}

pub(crate) fn explain_lint_rule(rule_id: &str) -> Option<LintRule> {
	Linter::new(
		lint_suites(None),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.find_rule(rule_id)
}

pub(crate) fn explain_lint_preset(preset_id: &str) -> Option<LintPreset> {
	Linter::new(
		lint_suites(None),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.find_preset(preset_id)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_check_command_with_configuration(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	fix: bool,
	ecosystems: &[String],
	only_rules: &[String],
	format: OutputFormat,
	verbose: bool,
	reporter: &crate::output::ProgressReporter,
) -> MonochangeResult<String> {
	let validation_started = std::time::Instant::now();
	reporter.phase_started("Validating workspace configuration");
	let (validation_warnings, validation_errors) =
		collect_workspace_validation_issues(root, configuration);
	reporter.phase_finished(
		"Validated workspace configuration",
		validation_started.elapsed(),
	);

	let selection = LintSelection::all()
		.with_suites(ecosystems.iter().cloned())
		.with_rules(only_rules.iter().cloned());
	let linter = build_linter(configuration, selection);

	let mut report = linter.lint_workspace(root, configuration, reporter);

	let mut fixed_files: Vec<(PathBuf, String)> = Vec::new();
	let mut fixed_file_count = 0usize;
	if fix {
		let fixes = linter.apply_fixes(&report);
		fixed_file_count = fixes.len();
		reporter.fix_started(fixed_file_count);
		for (file_path, fixed_content) in fixes {
			std::fs::write(&file_path, fixed_content).map_err(|error| {
				MonochangeError::Io(format!(
					"Failed to write fixed content to {}: {}",
					file_path.display(),
					error
				))
			})?;
			let description = report
				.autofixable()
				.iter()
				.find(|res| res.location.file_path == file_path)
				.and_then(|res| res.fix.as_ref())
				.map_or("fixed", |f| f.description.as_str());
			fixed_files.push((file_path.clone(), description.to_string()));
			reporter.fix_applied(&file_path, description);
		}
		reporter.fix_finished(fixed_files.len());

		if fixed_file_count > 0 {
			// NOTE: optimize later by re-linting only fixed files or adding a safe
			// fast-path for “all original errors were fixed.”
			report = linter.lint_workspace(
				root,
				configuration,
				&monochange_core::lint::NoopLintProgressReporter,
			);
		}
	}

	let lint_has_errors = report.has_errors();
	let validation_has_errors = !validation_errors.is_empty();
	let fixed_any_files = fixed_file_count > 0;
	reporter.summary(
		report.error_count,
		report.warning_count,
		report.autofixable().len(),
		fixed_any_files,
	);

	match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			let rendered = format
				.render_json_value(&report, "lint report")
				.unwrap_or_else(|error| panic!("serializing lint reports should succeed: {error}"));

			if validation_has_errors || lint_has_errors {
				let mut diagnostic = format!(
					"check failed: {} error{}, {} warning{}",
					report.error_count + validation_errors.len(),
					if report.error_count + validation_errors.len() == 1 {
						""
					} else {
						"s"
					},
					report.warning_count + validation_warnings.len(),
					if report.warning_count + validation_warnings.len() == 1 {
						""
					} else {
						"s"
					},
				);
				if validation_has_errors {
					diagnostic.push_str("\nworkspace validation failed");
					for error in &validation_errors {
						let _ = write!(diagnostic, "\n{error}");
					}
				}
				return Err(MonochangeError::Reported {
					output: rendered,
					diagnostic,
				});
			}

			Ok(rendered)
		}
		OutputFormat::Text | OutputFormat::Markdown => {
			let output = format_check_report(
				&CheckReportInput {
					root,
					report: &report,
					validation_warnings: &validation_warnings,
					validation_errors: &validation_errors,
					fixed_files: fixed_file_count,
					verbose,
				},
				TextTheme::for_stdout(),
			);
			if validation_has_errors || lint_has_errors {
				let diagnostic = format!(
					"check failed: {} error{}, {} warning{}",
					report.error_count + validation_errors.len(),
					if report.error_count + validation_errors.len() == 1 {
						""
					} else {
						"s"
					},
					report.warning_count + validation_warnings.len(),
					if report.warning_count + validation_warnings.len() == 1 {
						""
					} else {
						"s"
					},
				);
				Err(MonochangeError::Reported { output, diagnostic })
			} else {
				Ok(output)
			}
		}
	}
}

/// Run lint as part of a Validate step. Returns (`formatted_output`, `has_errors`).
#[allow(dead_code)]
pub(crate) fn run_lint_step(root: &Path, fix: bool) -> MonochangeResult<(String, bool)> {
	let configuration = load_workspace_configuration(root)?;
	let linter = build_linter(&configuration, LintSelection::all());
	let mut report = linter.lint_workspace(
		root,
		&configuration,
		&monochange_core::lint::NoopLintProgressReporter,
	);
	let mut fixed_file_count = 0usize;

	if fix {
		let fixes = linter.apply_fixes(&report);
		fixed_file_count = fixes.len();
		for (file_path, fixed_content) in fixes {
			std::fs::write(&file_path, fixed_content).map_err(|error| {
				MonochangeError::Io(format!(
					"Failed to write fixed content to {}: {}",
					file_path.display(),
					error
				))
			})?;
		}

		if fixed_file_count > 0 {
			// NOTE: keep this in sync with `run_check_command`'s post-fix
			// verification path.
			report = linter.lint_workspace(
				root,
				&configuration,
				&monochange_core::lint::NoopLintProgressReporter,
			);
		}
	}

	let has_errors = report.has_errors();
	Ok((
		format_check_report(
			&CheckReportInput {
				root,
				report: &report,
				validation_warnings: &[],
				validation_errors: &[],
				fixed_files: fixed_file_count,
				verbose: false,
			},
			TextTheme::for_stdout(),
		),
		has_errors,
	))
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn render_lint_catalog(format: OutputFormat) -> MonochangeResult<String> {
	let rules = available_lint_rules();
	let presets = available_lint_presets();
	match format {
		OutputFormat::Json | OutputFormat::JsonMin => {
			Ok(format
				.render_json_value(
					&serde_json::json!({
						"rules": rules,
						"presets": presets,
					}),
					"lint catalog",
				)
				.unwrap_or_else(|error| panic!("serializing lint catalog should succeed: {error}")))
		}
		OutputFormat::Text | OutputFormat::Markdown => {
			let mut text = TextReport::new(TextTheme::for_stdout());
			let fixable = rules.iter().filter(|rule| rule.autofixable).count();
			text.headline(
				Outcome::Neutral,
				&plural(rules.len(), "lint rule", "lint rules"),
				&[
					format!("{fixable} fixable"),
					plural(presets.len(), "preset", "presets"),
				],
			);
			text.section("Rules", Some(rules.len()));
			text.list(
				rules.iter().map(|rule| {
					let mut meta = format!(
						"{} · {}",
						debug_label(&rule.category),
						debug_label(&rule.maturity)
					);
					if rule.autofixable {
						meta.push_str(" · fixable");
					}
					format!("{}  {meta}\n{}", rule.id, rule.description)
				}),
				usize::MAX,
			);
			text.section("Presets", Some(presets.len()));
			text.list(
				presets.iter().map(|preset| {
					format!(
						"{}  {}\n{}",
						preset.id,
						debug_label(&preset.maturity),
						preset.description
					)
				}),
				usize::MAX,
			);
			text.paragraph(
				"Run `monochange lint explain <id>` for a rule's options or a preset's rules.",
				Tone::Muted,
			);
			Ok(text.render())
		}
	}
}

/// `Correctness` reads as `correctness` in result text.
fn debug_label(value: &impl std::fmt::Debug) -> String {
	format!("{value:?}").to_lowercase()
}

pub(crate) fn render_lint_explanation(id: &str, format: OutputFormat) -> MonochangeResult<String> {
	if let Some(rule) = explain_lint_rule(id) {
		return match format {
			OutputFormat::Json | OutputFormat::JsonMin => {
				format.render_json_value(&rule, "lint rule explanation")
			}
			OutputFormat::Text | OutputFormat::Markdown => {
				let mut text = TextReport::new(TextTheme::for_stdout());
				text.headline(Outcome::Neutral, &rule.id, std::slice::from_ref(&rule.name));
				text.fields(&[
					("Category", debug_label(&rule.category)),
					("Maturity", debug_label(&rule.maturity)),
					(
						"Autofix",
						if rule.autofixable { "yes" } else { "no" }.to_string(),
					),
				]);
				text.paragraph(&rule.description, Tone::Plain);
				if !rule.options.is_empty() {
					text.section("Options", Some(rule.options.len()));
					text.list(
						rule.options.iter().map(|option| {
							format!(
								"{}  {}\n{}",
								option.name,
								debug_label(&option.kind),
								option.description
							)
						}),
						usize::MAX,
					);
				}
				Ok(text.render())
			}
		};
	}

	if let Some(preset) = explain_lint_preset(id) {
		return match format {
			OutputFormat::Json | OutputFormat::JsonMin => {
				Ok(format
					.render_json_value(&preset, "lint preset explanation")
					.unwrap_or_else(|error| {
						panic!("serializing lint preset explanations should succeed: {error}")
					}))
			}
			OutputFormat::Text | OutputFormat::Markdown => {
				let mut text = TextReport::new(TextTheme::for_stdout());
				text.headline(
					Outcome::Neutral,
					&preset.id,
					std::slice::from_ref(&preset.name),
				);
				text.fields(&[("Maturity", debug_label(&preset.maturity))]);
				text.paragraph(&preset.description, Tone::Plain);
				text.section("Rules", Some(preset.rules.len()));
				let rows = preset
					.rules
					.iter()
					.map(|(rule_id, config)| {
						vec![
							TableCell::plain(rule_id),
							TableCell::new(config.severity().to_string(), Tone::Muted),
						]
					})
					.collect::<Vec<_>>();
				text.table(&rows);
				Ok(text.render())
			}
		};
	}

	Err(MonochangeError::Config(format!(
		"unknown lint rule or preset `{id}`"
	)))
}

pub(crate) fn handle_lint_subcommand(
	root: &Path,
	lint_matches: &ArgMatches,
) -> MonochangeResult<String> {
	let (subcommand, subcommand_matches) = lint_matches
		.subcommand()
		.expect("clap requires a lint subcommand");

	if subcommand == "list" {
		let format = subcommand_matches
			.get_one::<String>("format")
			.map_or(Ok(OutputFormat::Text), |value| {
				crate::parse_output_format(value)
			})?;
		return render_lint_catalog(format);
	}

	if subcommand == "explain" {
		let format = subcommand_matches
			.get_one::<String>("format")
			.map_or(Ok(OutputFormat::Text), |value| {
				crate::parse_output_format(value)
			})?;
		let id = subcommand_matches
			.get_one::<String>("id")
			.expect("clap requires a lint id")
			.as_str();
		return render_lint_explanation(id, format);
	}

	let id = subcommand_matches
		.get_one::<String>("id")
		.expect("clap requires a lint id")
		.as_str();
	scaffold_lint_rule(root, id)
}

pub(crate) fn scaffold_lint_rule(root: &Path, id: &str) -> MonochangeResult<String> {
	let (suite, rule_name) = id.split_once('/').ok_or_else(|| {
		MonochangeError::Config("lint ids must use the form <ecosystem>/<rule-name>".to_string())
	})?;
	let crate_name = match suite {
		"cargo" => "monochange_cargo",
		"npm" => "monochange_npm",
		"dart" => "monochange_dart",
		other => {
			return Err(MonochangeError::Config(format!(
				"scaffolding is not yet supported for lint suite `{other}`"
			)));
		}
	};
	let module_name = rule_name.replace('-', "_");
	let struct_name = rule_name
		.split('-')
		.map(|segment| {
			let mut chars = segment.chars();
			match chars.next() {
				Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
				None => String::new(),
			}
		})
		.collect::<String>()
		+ "Rule";

	let lint_dir = root.join("crates").join(crate_name).join("src/lints");
	std::fs::create_dir_all(&lint_dir).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create lint directory {}: {error}",
			lint_dir.display()
		))
	})?;
	let lint_file = lint_dir.join(format!("{module_name}.rs"));
	if lint_file.exists() {
		return Err(MonochangeError::Config(format!(
			"lint file {} already exists",
			lint_file.display()
		)));
	}

	let fixture_dir = root
		.join("fixtures/tests/lints")
		.join(suite)
		.join(rule_name)
		.join("workspace");
	std::fs::create_dir_all(&fixture_dir).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create lint fixture directory {}: {error}",
			fixture_dir.display()
		))
	})?;

	let template = format!(
		r#"use monochange_core::lint::LintContext;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_linting::declare_lint_rule;
use monochange_linting::LintCategory;
use monochange_linting::LintMaturity;

// Keep `declare_lint_rule!` for straightforward metadata-only construction.
// If this rule later needs extra constructor state, switch to an explicit
// `struct` plus `LintRule::new(...)`.
declare_lint_rule! {{
    pub {struct_name},
    id: "{suite}/{rule_name}",
    name: "TODO: rename me",
    description: "TODO: describe what this lint checks",
    category: LintCategory::BestPractice,
    maturity: LintMaturity::Experimental,
    autofixable: false,
}}

impl LintRuleRunner for {struct_name} {{
    fn rule(&self) -> &LintRule {{
        &self.rule
    }}

    fn run(&self, _ctx: &LintContext<'_>, _config: &LintRuleConfig) -> Vec<LintResult> {{
        Vec::new()
    }}
}}
"#
	);
	std::fs::write(&lint_file, template).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write lint file {}: {error}",
			lint_file.display()
		))
	})?;

	let note_file = fixture_dir.join("README.md");
	std::fs::write(
		&note_file,
		format!("# {id}\n\nAdd fixture workspaces for snapshot and autofix tests here.\n"),
	)
	.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write fixture note {}: {error}",
			note_file.display()
		))
	})?;

	Ok(format!(
		"Created {} and {}.\nNext steps:\n- wire `mod {module_name};` into `crates/{crate_name}/src/lints/mod.rs`\n- register `{struct_name}::new()` in the suite\n- add fixture scenarios under {}",
		lint_file.display(),
		note_file.display(),
		fixture_dir.display()
	))
}

/// Everything `monochange check` reports in its human result.
struct CheckReportInput<'a> {
	root: &'a Path,
	report: &'a LintReport,
	validation_warnings: &'a [String],
	validation_errors: &'a [String],
	fixed_files: usize,
	verbose: bool,
}

fn format_check_report(input: &CheckReportInput<'_>, theme: TextTheme) -> String {
	let report = input.report;
	let errors = report.error_count + input.validation_errors.len();
	let warnings = report.warning_count + input.validation_warnings.len() + report.warnings.len();
	let fixable = report.autofixable().len();
	let mut text = TextReport::new(theme);

	let mut details = Vec::new();
	if errors > 0 {
		details.push(plural(errors, "error", "errors"));
	}
	if warnings > 0 {
		details.push(plural(warnings, "warning", "warnings"));
	}
	if fixable > 0 {
		details.push(format!("{fixable} fixable"));
	}
	if input.fixed_files > 0 {
		details.push(format!(
			"fixed {}",
			plural(input.fixed_files, "file", "files")
		));
	}
	let (outcome, headline) = match (errors, warnings) {
		(0, 0) => (Outcome::Success, "Checks passed"),
		(0, _) => (Outcome::Warning, "Checks passed with warnings"),
		_ => (Outcome::Failure, "Checks failed"),
	};
	text.headline(outcome, headline, &details);

	if !input.validation_errors.is_empty() {
		text.section("Workspace validation", Some(input.validation_errors.len()));
		for error in input.validation_errors {
			text.raw_block(error);
		}
	}

	let workspace_warnings = input
		.validation_warnings
		.iter()
		.chain(&report.warnings)
		.collect::<Vec<_>>();
	if !workspace_warnings.is_empty() {
		text.section("Warnings", Some(workspace_warnings.len()));
		text.list(
			workspace_warnings
				.into_iter()
				.map(|warning| format!("▲ {warning}")),
			usize::MAX,
		);
	}

	let mut by_file: BTreeMap<&Path, Vec<&monochange_core::lint::LintResult>> = BTreeMap::new();
	for result in &report.results {
		by_file
			.entry(&result.location.file_path)
			.or_default()
			.push(result);
	}
	for (file, mut results) in by_file {
		results.sort_by(|left, right| {
			(left.location.line, left.location.column, &left.rule_id).cmp(&(
				right.location.line,
				right.location.column,
				&right.rule_id,
			))
		});
		text.paragraph(
			&root_relative(input.root, file).display().to_string(),
			Tone::Heading,
		);
		let location_width = results
			.iter()
			.map(|result| format!("{}:{}", result.location.line, result.location.column).len())
			.max()
			.unwrap_or_default();
		for result in results {
			let (icon, tone) = match result.severity {
				LintSeverity::Error => ("✖", Tone::Error),
				LintSeverity::Warning => ("▲", Tone::Warning),
				LintSeverity::Off => ("·", Tone::Muted),
			};
			let location = format!("{}:{}", result.location.line, result.location.column);
			let padding = " ".repeat(location_width - location.len());
			text.indented(
				&format!(
					"{}{padding}  {} {}",
					theme.paint(&location, Tone::Muted),
					theme.paint(icon, tone),
					result.message
				),
				Tone::Plain,
			);
			let mut rule = vec![result.rule_id.clone()];
			if result.fix.is_some() {
				rule.push("fixable".to_string());
			}
			let indent = " ".repeat(location_width + 4);
			text.indented(&format!("{indent}{}", rule.join(" · ")), Tone::Muted);
			if input.verbose {
				let mut verbose = vec![format!("severity {}", result.severity)];
				if let Some((start, end)) = result.location.span {
					verbose.push(format!("span {start}..{end}"));
				}
				if let Some(fix) = result.fix.as_ref() {
					verbose.push(format!("fix: {}", fix.description));
				}
				text.indented(&format!("{indent}{}", verbose.join(" · ")), Tone::Muted);
			}
		}
	}

	if fixable > 0 {
		let command = theme.paint("`monochange check --fix`", Tone::Accent);
		let sentence = if input.fixed_files > 0 {
			format!(
				"{} still auto-fixable; run {command} again.",
				plural(fixable, "issue is", "issues are")
			)
		} else {
			format!(
				"Run {command} to fix {} automatically.",
				plural(fixable, "issue", "issues")
			)
		};
		text.paragraph(&sentence, Tone::Plain);
	}

	text.render()
}

#[cfg(test)]
#[path = "__tests__/lint_tests.rs"]
mod tests;
