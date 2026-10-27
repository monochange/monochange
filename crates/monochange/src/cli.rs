use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;
use std::path::PathBuf;

use clap::Arg;
use clap::ArgAction;
use clap::ColorChoice;
use clap::Command;
use monochange_config::load_cli_commands;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::MonochangeError;
use monochange_core::default_cli_commands;

/// Build the top-level Clap command for the `monochange` binary.
///
/// The returned command includes built-in subcommands such as `init`, `subagents`,
/// `mcp`, and `release-record`, plus any config-defined commands resolved from
/// the current working directory.
pub fn build_command(bin_name: &'static str) -> Command {
	let root = current_dir_or_dot();
	build_command_for_root(bin_name, &root)
}

pub(crate) fn configured_change_type_choices(
	configuration: &monochange_core::WorkspaceConfiguration,
) -> Vec<String> {
	configuration
		.changelog
		.types
		.keys()
		.cloned()
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

pub(crate) fn apply_runtime_change_type_choices(
	cli: &mut [CliCommandDefinition],
	configuration: &monochange_core::WorkspaceConfiguration,
) {
	let choices = configured_change_type_choices(configuration);

	if choices.is_empty() {
		return;
	}

	let Some(change_command) = cli.iter_mut().find(|command| command.name == "change") else {
		return;
	};
	let Some(change_type_input) = change_command
		.inputs
		.iter_mut()
		.find(|input| input.name == "type" && input.choices.is_empty())
	else {
		return;
	};

	change_type_input.kind = CliInputKind::Choice;
	change_type_input.choices = choices;
}

pub(crate) fn cli_commands_for_root(root: &Path) -> Vec<CliCommandDefinition> {
	load_cli_commands(root).unwrap_or_else(|_| default_cli_commands())
}

/// Extract CLI commands from an already-loaded configuration result, avoiding
/// a redundant config load when the caller has already parsed the config.
pub(crate) fn cli_commands_from_config(
	configuration: &Result<monochange_core::WorkspaceConfiguration, MonochangeError>,
) -> Vec<CliCommandDefinition> {
	let Ok(configuration) = configuration else {
		return default_cli_commands();
	};

	let mut cli = configuration.cli.clone();
	apply_runtime_change_type_choices(&mut cli, configuration);
	apply_runtime_prepare_release_output_formats(&mut cli);

	cli
}

pub(crate) fn apply_runtime_prepare_release_output_formats(cli: &mut [CliCommandDefinition]) {
	for cli_command in cli {
		if !command_supports_release_diff_preview(cli_command) {
			continue;
		}

		let Some(format_input) = cli_command
			.inputs
			.iter_mut()
			.find(|input| input.name == "format")
		else {
			continue;
		};

		let has_markdown = format_input
			.choices
			.iter()
			.any(|choice| choice == "markdown");

		if !has_markdown {
			format_input.choices.insert(0, "markdown".to_string());
		}

		let has_md = format_input.choices.iter().any(|choice| choice == "md");

		if !has_md {
			format_input.choices.push("md".to_string());
		}
	}
}

pub(crate) fn build_command_for_root(bin_name: &'static str, root: &Path) -> Command {
	let cli = cli_commands_for_root(root);
	build_command_with_cli(bin_name, &cli)
}

/// Color theme for monochange CLI help output.
fn monochange_styles() -> clap::builder::Styles {
	clap::builder::Styles::styled()
		.header(crate::cli_theme::header())
		.usage(crate::cli_theme::usage())
		.literal(crate::cli_theme::literal())
		.placeholder(crate::cli_theme::placeholder())
		.error(crate::cli_theme::error())
		.valid(crate::cli_theme::valid())
		.invalid(crate::cli_theme::error())
}

const GLOBAL_OPTIONS_HELP_HEADING: &str = "Global Options";
const RELEASE_OPTIONS_HELP_HEADING: &str = "Release Options";
const ROOT_LONG_ABOUT: &str = "monochange discovers packages across Cargo, npm/pnpm/Bun, Deno, Dart/Flutter, Python, and Go, then coordinates version bumps, changelogs, and release automation from a single monochange.toml config.";
const ROOT_AFTER_LONG_HELP_MARKDOWN: &str = include_str!("cli_after_long_help.md");

fn root_after_long_help() -> String {
	markdown_to_clap_help(ROOT_AFTER_LONG_HELP_MARKDOWN)
}

fn markdown_to_clap_help(markdown: &str) -> String {
	let mut output = String::new();
	let mut in_code_block = false;
	let mut previous_blank = true;

	for line in markdown.lines() {
		let trimmed = line.trim();

		if trimmed.starts_with("<!--") && trimmed.ends_with("-->") {
			continue;
		}

		if trimmed.starts_with("```") {
			in_code_block = !in_code_block;
			continue;
		}

		if trimmed.is_empty() {
			if !previous_blank {
				output.push('\n');
				previous_blank = true;
			}

			continue;
		}

		let parsed = if in_code_block {
			format!("  {trimmed}")
		} else if let Some(heading) = trimmed.strip_prefix("### ") {
			strip_inline_markdown(heading)
		} else if let Some(heading) = trimmed.strip_prefix("## ") {
			strip_inline_markdown(heading)
		} else if let Some(heading) = trimmed.strip_prefix("# ") {
			strip_inline_markdown(heading)
		} else {
			strip_inline_markdown(trimmed)
		};

		output.push_str(&parsed);
		output.push('\n');
		previous_blank = false;
	}

	output.trim_end().to_string()
}

fn strip_inline_markdown(value: &str) -> String {
	let mut output = String::new();
	let mut chars = value.chars().peekable();

	while let Some(character) = chars.next() {
		match character {
			'`' => {}
			'[' => {
				let mut label = String::new();
				for next in chars.by_ref() {
					if next == ']' {
						break;
					}

					label.push(next);
				}
				if chars.peek() == Some(&'(') {
					chars.next();

					for next in chars.by_ref() {
						if next == ')' {
							break;
						}
					}
				}

				output.push_str(&label);
			}
			_ => output.push(character),
		}
	}

	output
}

pub(crate) fn build_command_with_cli(
	bin_name: &'static str,
	cli: &[CliCommandDefinition],
) -> Command {
	build_command_with_workflows(bin_name, cli, true)
}

/// Build the command a CLI surface snapshot describes.
///
/// A snapshot records the binary's own command surface, so workflow commands
/// from monochange.toml appear only under `run`, unless the requested path
/// starts with one of them.
pub(crate) fn build_snapshot_command(
	bin_name: &'static str,
	cli: &[CliCommandDefinition],
	path: &[String],
) -> Command {
	let names_workflow = path
		.first()
		.is_some_and(|first| cli.iter().any(|cli_command| cli_command.name == *first));
	build_command_with_workflows(bin_name, cli, names_workflow)
}

#[allow(clippy::redundant_closure_for_method_calls)]
fn build_command_with_workflows(
	bin_name: &'static str,
	cli: &[CliCommandDefinition],
	top_level_workflows: bool,
) -> Command {
	let mut command = Command::new(bin_name)
		.version(env!("CARGO_PKG_VERSION"))
		.about("Manage versions and releases for your multiplatform, multilanguage monorepo")
		.long_about(ROOT_LONG_ABOUT)
		.after_long_help(root_after_long_help())
		.styles(monochange_styles())
		.color(ColorChoice::Auto)
		.disable_help_subcommand(true)
		.subcommand_required(true)
		.arg_required_else_help(true)
		.next_help_heading(GLOBAL_OPTIONS_HELP_HEADING)
		.arg(
			Arg::new("log-level")
				.long("log-level")
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help("Set tracing filter (e.g. debug, monochange=trace)")
				.value_name("FILTER")
				.hide(true),
		)
		.arg(
			Arg::new("quiet")
				.long("quiet")
				.short('q')
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help("Suppress result, progress, and diagnostic output without changing execution")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("verbose")
				.long("verbose")
				.short('v')
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help("Show full result lists and logs, every step timing, and progress notes")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("color")
				.long("color")
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help("Control when output uses color; `auto` honors NO_COLOR, CLICOLOR, and CLICOLOR_FORCE")
				.value_name("WHEN")
				.value_parser(["auto", "always", "never"]),
		)
		.arg(
			Arg::new("progress-format")
				.long("progress-format")
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help("Control progress output on stderr")
				.value_name("FORMAT")
				.value_parser(["auto", "unicode", "ascii", "json"]),
		)
		.arg(
			Arg::new("jq")
				.long("jq")
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help(
					"Filter explicit JSON output; requires `--format json` or `--format json-min`",
				)
				.value_name("EXPRESSION"),
		)
		.arg(
			Arg::new("snapshot")
				.long("snapshot")
				.global(true)
				.help_heading(GLOBAL_OPTIONS_HELP_HEADING)
				.help("Print a normalized JSON snapshot for this command subtree")
				.action(ArgAction::SetTrue),
		)
		.subcommand(
			Command::new("snapshot")
				.about("Print a normalized JSON snapshot of the CLI surface")
				.arg(
					Arg::new("view")
						.long("view")
						.value_name("VIEW")
						.default_value("full")
						.value_parser(["full", "light", "index"])
						.help("Snapshot view to render"),
				)
				.arg(
					Arg::new("command")
						.value_name("COMMAND")
						.num_args(0..)
						.help("Optional command path to render"),
				)
				.arg(
					Arg::new("package")
						.long("package")
						.value_name("PACKAGE")
						.help(
							"Capture the configured cli snapshot for this package instead of the monochange surface",
						),
				)
				.arg(
					Arg::new("save")
						.long("save")
						.action(ArgAction::SetTrue)
						.help(
							"Write the captured snapshot as the committed baseline under .monochange/cli-snapshots/",
						),
				)
				.arg(
					Arg::new("list")
						.long("list")
						.action(ArgAction::SetTrue)
						.help("List registered package clis and their baseline status"),
				),
		)
		.subcommand(
			Command::new("init")
				.about("Generate monochange.toml with detected packages and groups")
				.arg(
					Arg::new("force")
						.long("force")
						.help("Overwrite an existing monochange.toml file")
						.action(ArgAction::SetTrue),
				)
				.arg(
					Arg::new("provider")
						.long("provider")
						.value_name("PROVIDER")
						.help("Source-control provider for release automation workflows")
						.value_parser(["github", "gitlab", "gitea", "forgejo"]),
				)
				.after_help(
					"Examples:\n  monochange init\n  monochange init --provider github\n  monochange init --force\n\n\
With --provider, the generated config also includes:\n  - a [source] section for the provider\n  - \
release and pull request settings for the provider\n  - GitHub Actions workflows (for --provider github)\n\n\
The starter config does not generate [cli.*] command aliases.",
				),
		)
		.subcommand(build_command_wizard_subcommand())
		.subcommand(build_skill_subcommand())
		.subcommand(build_subagents_subcommand())
		.subcommand(build_change_subcommand())
		.subcommand(build_api_subcommand())
		.subcommand(build_changeset_subcommand())
		.subcommand(build_analyze_subcommand())
		.subcommand(build_notes_subcommand())
		.subcommand(build_migrate_subcommand())
		.subcommand(build_lint_subcommand())
		.subcommand(build_versions_subcommand())
		.subcommand(build_publish_subcommand())
		.subcommands(build_top_level_step_alias_subcommands())
		.subcommand({
			#[cfg(feature = "mcp")]
			{
				Command::new("mcp")
					.about("Start the monochange MCP (Model Context Protocol) server over stdin/stdout")
					.after_help(
						"Examples:\n  monochange mcp\n\nAgent runtimes launch this command themselves; `monochange subagents` \
writes the MCP config that does so.",
					)
			}
			#[cfg(not(feature = "mcp"))]
			{
				Command::new("mcp").hide(true)
			}
		})
		.subcommand(build_check_subcommand())
		.subcommand(build_completions_command())
		.subcommand(build_doctor_command())
		.subcommand(build_help_subcommand());

	command = command
		.subcommand(build_step_subcommand())
		.subcommand(build_run_subcommand(cli));

	// Commands defined in monochange.toml also run at the top level, as
	// `monochange <name>`, unless a built-in command already owns the name.
	let builtin_names = command
		.get_subcommands()
		.map(|subcommand| subcommand.get_name().to_string())
		.collect::<BTreeSet<_>>();
	let top_level = cli
		.iter()
		.filter(|cli_command| top_level_workflows && !builtin_names.contains(&cli_command.name));

	for cli_command in top_level {
		command = command.subcommand(build_cli_command_subcommand_with_prefix(
			cli_command,
			"monochange",
		));
	}

	let template = root_help_template(&command, cli, &builtin_names);
	command.help_template(template)
}

/// Built-in commands grouped by task for the root help, in display order.
const HELP_COMMAND_GROUPS: &[(&str, &[&str])] = &[
	(
		"Release Commands",
		&[
			"next", "create", "preview", "prepare", "publish", "affected", "diagnose",
		],
	),
	(
		"Workspace Commands",
		&[
			"init", "discover", "config", "check", "versions", "lint", "migrate", "doctor",
		],
	),
	(
		"Change Analysis Commands",
		&["change", "changeset", "api", "analyze", "notes"],
	),
	(
		"Automation Commands",
		&[
			"run",
			"step",
			"command",
			"mcp",
			"skill",
			"subagents",
			"snapshot",
			"completions",
			"help",
		],
	),
];

/// Render the root help template: built-in commands grouped by task, then the
/// commands defined in monochange.toml, then the options.
fn root_help_template(
	command: &Command,
	cli: &[CliCommandDefinition],
	builtin_names: &BTreeSet<String>,
) -> String {
	let visible = command
		.get_subcommands()
		.filter(|subcommand| !subcommand.is_hide_set())
		.filter(|subcommand| builtin_names.contains(subcommand.get_name()))
		.map(|subcommand| {
			(
				subcommand.get_name().to_string(),
				subcommand
					.get_about()
					.map(ToString::to_string)
					.unwrap_or_default(),
			)
		})
		.collect::<Vec<_>>();
	let mut sections = Vec::new();
	let mut grouped = BTreeSet::new();

	for (heading, names) in HELP_COMMAND_GROUPS {
		let rows = names
			.iter()
			.filter_map(|name| visible.iter().find(|(visible, _)| visible == name))
			.cloned()
			.collect::<Vec<_>>();
		grouped.extend(names.iter().map(ToString::to_string));
		sections.push(((*heading).to_string(), rows));
	}

	let ungrouped = visible
		.iter()
		.filter(|(name, _)| !grouped.contains(name))
		.cloned()
		.collect::<Vec<_>>();
	sections.push(("Other Commands".to_string(), ungrouped));
	let workflows = cli
		.iter()
		.map(|cli_command| {
			let about = cli_command
				.help_text
				.clone()
				.unwrap_or_else(|| format!("Run the `{}` command", cli_command.name));
			if builtin_names.contains(&cli_command.name) {
				let name = &cli_command.name;
				(
					name.clone(),
					format!("{about} (run it with `monochange run {name}`)"),
				)
			} else {
				(cli_command.name.clone(), about)
			}
		})
		.collect::<Vec<_>>();
	sections.push(("Workflow Commands (monochange.toml)".to_string(), workflows));

	let width = sections
		.iter()
		.flat_map(|(_, rows)| rows.iter().map(|(name, _)| name.chars().count()))
		.max()
		.unwrap_or_default();
	let header = crate::cli_theme::header();
	let literal = crate::cli_theme::literal();
	let mut template = String::from("{before-help}{about-with-newline}\n{usage-heading} {usage}");

	for (heading, rows) in sections.iter().filter(|(_, rows)| !rows.is_empty()) {
		let _ = write!(template, "\n\n{header}{heading}:{header:#}");

		for (name, about) in rows {
			let padding = " ".repeat(width - name.chars().count());
			let _ = write!(template, "\n  {literal}{name}{literal:#}{padding}  {about}");
		}
	}

	let _ = write!(
		template,
		"\n\n{header}Options:{header:#}\n{{options}}{{after-help}}"
	);
	template
}

pub(crate) fn build_change_subcommand() -> Command {
	Command::new("change")
		.about("Determine the release severity of repository changes")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(build_classify_subcommand())
}

fn build_classify_subcommand() -> Command {
	Command::new("classify")
		.about("Compare the pull request and latest release, then propose package bumps")
		.after_help(
			"Compares a merge candidate with its base branch and each package's latest release. Reports compatibility impact, proposed changeset bump, confidence, completeness, and the findings that support each recommendation.",
		)
		.arg(
			Arg::new("base")
				.long("base")
				.value_name("REF")
				.help("Base branch to compare with; defaults to the remote default branch, so pass the pull request base for a stacked branch"),
		)
		.arg(
			Arg::new("head")
				.long("head")
				.value_name("REF")
				.default_value("HEAD")
				.help("Pull request head ref to classify"),
		)
		.arg(classification_release_arg())
		.arg(classification_package_arg())
		.arg(classification_label_arg())
		.arg(classification_detection_level_arg())
		.arg(classification_include_unchanged_arg())
		.arg(classification_skip_cli_snapshots_arg())
		.arg(classification_format_arg())
		.arg(classification_output_arg())
		.arg(classification_dependency_propagation_arg())
}

pub(crate) fn build_api_subcommand() -> Command {
	Command::new("api")
		.about("Inspect lower-level API diffs")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("diff")
				.about("Classify an explicit API diff")
				.after_help(
					"Examples:\n  monochange api diff --package core\n  monochange api diff --base origin/main --head HEAD --format json",
				)
				.arg(
					Arg::new("base")
						.long("base")
						.value_name("REF")
						.help("Base ref; defaults to the remote default branch"),
				)
				.arg(
					Arg::new("head")
						.long("head")
						.value_name("REF")
						.default_value("HEAD")
						.help("Head ref to classify"),
				)
				.arg(classification_release_arg())
				.arg(classification_package_arg())
				.arg(classification_label_arg())
				.arg(classification_detection_level_arg())
				.arg(classification_include_unchanged_arg())
				.arg(classification_skip_cli_snapshots_arg())
				.arg(classification_format_arg())
				.arg(classification_output_arg())
				.arg(classification_dependency_propagation_arg()),
		)
}

pub(crate) fn build_changeset_subcommand() -> Command {
	Command::new("changeset")
		.about("Inspect and validate pending changeset intent")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("validate")
				.about("Compare pending changesets with classified change severity")
				.arg(
					Arg::new("api")
						.long("api")
						.required(true)
						.action(ArgAction::SetTrue)
						.help("Validate against semantic change classification"),
				)
				.arg(
					Arg::new("base")
						.long("base")
						.value_name("REF")
						.help("Override the automatically detected default branch ref"),
				)
				.arg(
					Arg::new("head")
						.long("head")
						.value_name("REF")
						.default_value("HEAD")
						.help("Head ref to classify"),
				)
				.arg(classification_release_arg())
				.arg(classification_package_arg())
				.arg(classification_detection_level_arg())
				.arg(classification_include_unchanged_arg())
				.arg(classification_skip_cli_snapshots_arg())
				.arg(
					Arg::new("strict")
						.long("strict")
						.action(ArgAction::SetTrue)
						.help("Fail on any proposed bump mismatch, including incomplete evidence"),
				)
				.arg(classification_format_arg())
				.arg(classification_output_arg())
				.arg(classification_dependency_propagation_arg()),
		)
}

fn classification_release_arg() -> Arg {
	Arg::new("release")
		.long("release")
		.value_name("REF")
		.help("Override the latest release ref for every selected package")
}

fn classification_package_arg() -> Arg {
	Arg::new("package")
		.long("package")
		.value_name("PACKAGE")
		.action(ArgAction::Append)
		.help("Limit classification to a package id or name; repeat to select several")
}

fn classification_detection_level_arg() -> Arg {
	Arg::new("detection-level")
		.long("detection-level")
		.value_name("LEVEL")
		.default_value("signature")
		.value_parser(["basic", "signature", "semantic"])
		.help("Choose analyzer depth")
}

fn classification_include_unchanged_arg() -> Arg {
	Arg::new("include-unchanged")
		.long("include-unchanged")
		.action(ArgAction::SetTrue)
		.help("Include packages without findings")
}

fn classification_label_arg() -> Arg {
	Arg::new("label")
		.long("label")
		.value_name("LABEL")
		.action(ArgAction::Append)
		.help("Pull request label to compare against [changesets.classification].skip_labels; repeat to pass several")
}

fn classification_skip_cli_snapshots_arg() -> Arg {
	Arg::new("skip-cli-snapshots")
		.long("skip-cli-snapshots")
		.action(ArgAction::SetTrue)
		.help("Skip registered cli command-surface comparisons; MONOCHANGE_SKIP_CLI_SNAPSHOTS=1 also skips them")
}

fn classification_output_arg() -> Arg {
	Arg::new("output")
		.long("output")
		.value_name("PATH")
		.help("Also write the report to a file")
}

fn classification_format_arg() -> Arg {
	format_arg(MARKDOWN_OUTPUT_FORMATS)
}

/// Formats for commands that also render Markdown; `markdown` is accepted as a
/// spelling of `md`.
const MARKDOWN_OUTPUT_FORMATS: &[&str] = &["text", "json", "json-min", "md"];
/// Formats for commands whose results are text or data only.
const DATA_OUTPUT_FORMATS: &[&str] = &["text", "json", "json-min"];

/// The `--format` option, described the same way on every command.
fn format_arg(formats: &'static [&'static str]) -> Arg {
	let possible_values = formats.iter().map(|format| {
		clap::builder::PossibleValue::new(*format).aliases((*format == "md").then_some("markdown"))
	});
	Arg::new("format")
		.long("format")
		.value_name("FORMAT")
		.help("Output format for the command result")
		.default_value("text")
		.value_parser(clap::builder::PossibleValuesParser::new(possible_values))
}

fn classification_dependency_propagation_arg() -> Arg {
	Arg::new("dependency-propagation")
		.long("dependency-propagation")
		.value_name("MODE")
		.default_value("none")
		.value_parser(["none", "public"])
		.help("Propose patch bumps for direct public runtime dependents")
}

pub(crate) fn build_notes_subcommand() -> Command {
	Command::new("notes")
		.about("Render one configured release-note output without modifying release files")
		.after_help(
			r"Examples:
  monochange notes --output user_notes
  monochange notes --output user_notes --target app
  monochange notes --output user_notes --target app --file release-notes.md
  monochange notes --output user_notes --target app --file -

Selection rules:
  - --output names `default` or a [changelog.outputs.<id>] entry.
  - The selected output supplies the stream and format; this command does not override either.
  - Use --target when an output produces notes for more than one package or group.
  - stdout is the default destination. --file writes only that explicit path.
  - Rendering is read-only: changesets, versions, and configured changelog paths are not modified.",
		)
		.arg(
			Arg::new("output")
				.long("output")
				.required(true)
				.value_name("OUTPUT")
				.help("Configured changelog output to render"),
		)
		.arg(
			Arg::new("target")
				.long("target")
				.value_name("TARGET")
				.help("Package or group id to select from a multi-target output"),
		)
		.arg(
			Arg::new("file")
				.long("file")
				.value_name("PATH")
				.allow_hyphen_values(true)
				.help("Write notes to this path instead of stdout; use - for stdout"),
		)
}

pub(crate) fn build_command_wizard_subcommand() -> Command {
	Command::new("command")
		.about("Open an interactive dashboard for adding or editing config-defined CLI commands")
		.after_help(
			"Examples:\n  monochange command\n\nUse this wizard to create or revise [cli.<name>] entries in monochange.toml. It keeps existing command details unless you choose to replace them.",
		)
}

pub(crate) fn build_skill_subcommand() -> Command {
	Command::new("skill")
		.about("Read or install the bundled monochange agent skill")
		.after_help(
			r"Examples:
  monochange skill
  monochange skill read monochange
  monochange skill read configuration
  monochange skill install --dir ./.claude/skills/monochange
  monochange skill install --dir ~/.codex/skills/monochange --force

The skill ships inside this binary, so it works without network access, an npm
package, or a cloned repository. Bare `monochange skill` lists every bundled
topic. The skill directory is normally named `monochange` inside a runtime's
skills folder. Existing skills are never replaced unless --force is passed.",
		)
		.subcommand(
			Command::new("read")
				.about("Print a bundled skill topic as raw Markdown")
				.after_help(
					"The output is the document itself, with no terminal rendering or added framing, \
					 so YAML frontmatter and code blocks survive verbatim. Omit TOPIC to list every \
					 bundled topic.",
				)
				.arg(
					Arg::new("topic")
						.value_name("TOPIC")
						.help("Topic to print; omit to list every bundled topic"),
				),
		)
		.subcommand(
			Command::new("install")
				.about("Write the bundled skill into an agent runtime's skill directory")
				.after_help(
					"Writes SKILL.md and every bundled reference. The directory is normally named \
					 `monochange` inside the runtime's skills folder. An existing skill is never \
					 replaced unless --force is passed.",
				)
				.arg(
					Arg::new("dir")
						.long("dir")
						.value_name("DIR")
						.help("Skill directory to write, for example ~/.claude/skills/monochange"),
				)
				.arg(
					Arg::new("force")
						.long("force")
						.action(ArgAction::SetTrue)
						.help("Replace an existing skill in the destination"),
				),
		)
}

pub(crate) fn build_subagents_subcommand() -> Command {
	Command::new("subagents")
		.about("Generate repo-local monochange subagents and agent guidance files")
		.after_help(
			r"Examples:
  monochange help subagents
  monochange subagents claude
  monochange subagents pi codex
  monochange subagents --all --dry-run --format json
  monochange subagents vscode copilot --no-mcp

Targets:
  - claude  -> .claude/agents/*.md and .mcp.json
  - vscode  -> .github/agents/*.agent.md and .vscode/mcp.json
  - copilot -> .github/agents/*.agent.md and .vscode/mcp.json
  - pi      -> .pi/agents/*.md
  - codex   -> .codex/agents/*.toml
  - cursor  -> .cursor/rules/*.mdc

Generated agents are CLI-first. They should prefer:
  1. monochange
  2. npx -y @monochange/cli

Use `--no-mcp` to skip MCP config files for targets that support repo-local MCP config.",
		)
		.arg(
			Arg::new("target")
				.help("Subagent target(s) to generate")
				.value_name("TARGET")
				.num_args(1..)
				.required_unless_present("all")
				.value_parser(["claude", "vscode", "copilot", "pi", "codex", "cursor"]),
		)
		.arg(
			Arg::new("all")
				.long("all")
				.help("Generate files for all supported targets")
				.conflicts_with("target")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("force")
				.long("force")
				.help("Overwrite generated files that already exist with different contents")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Preview the generated files without writing them")
				.action(ArgAction::SetTrue),
		)
		.arg(format_arg(MARKDOWN_OUTPUT_FORMATS))
		.arg(
			Arg::new("no-mcp")
				.long("no-mcp")
				.help("Skip repo-local MCP config files for supported targets")
				.action(ArgAction::SetTrue),
		)
}

pub(crate) fn build_analyze_subcommand() -> Command {
	Command::new("analyze")
		.about("Analyze semantic changes for one package across main, head, and optional release baselines")
		.after_help(
			r"Examples:
  monochange analyze --package core
  monochange analyze --package core --format json
  monochange analyze --package core --release-ref core/v1.2.3
  monochange analyze --package core --main-ref main --head-ref HEAD

Analysis notes:
  - Runs package-scoped semantic analysis using the selected package's configured release identity.
  - Defaults `--release-ref` to the newest tag for the package or the version group that owns it.
  - If no prior release tag exists, falls back to first-release analysis using only `main -> head`.",
		)
		.arg(
			Arg::new("package")
				.long("package")
				.required(true)
				.value_name("PACKAGE")
				.help("Configured package id, discovered package id, package name, manifest path, or package directory"),
		)
		.arg(
			Arg::new("release-ref")
				.long("release-ref")
				.value_name("REF")
				.help("Explicit release baseline ref. Defaults to the latest tag for the package or owning version group"),
		)
		.arg(
			Arg::new("main-ref")
				.long("main-ref")
				.value_name("REF")
				.help("Base branch or ref to compare against. Defaults to the detected default branch"),
		)
		.arg(
			Arg::new("head-ref")
				.long("head-ref")
				.value_name("REF")
				.help("Head ref to analyze. Defaults to HEAD"),
		)
		.arg(
			Arg::new("detection-level")
				.long("detection-level")
				.value_name("LEVEL")
				.default_value("signature")
				.value_parser(["basic", "signature", "semantic"])
				.help("Level of semantic detail to request from analyzers"),
		)
		.arg(
			format_arg(MARKDOWN_OUTPUT_FORMATS),
		)
}

pub(crate) fn build_migrate_subcommand() -> Command {
	Command::new("migrate")
		.about("Audit or migrate release metadata for monochange repositories")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("audit")
				.about("Report existing release tools, changelog providers, and CI migration work")
				.after_help(
					r"Examples:
  monochange migrate audit
  monochange migrate audit --format json

Audit notes:
  - Looks for known release tools such as knope, Changesets, release-please, semantic-release, and cargo-release.
  - Scans GitHub Actions workflows for legacy release actions and commands.
  - Reports changelog providers and trusted-publishing migration recommendations.",
				)
				.arg(
					format_arg(MARKDOWN_OUTPUT_FORMATS),
				),
		)
		.subcommand(
			Command::new("release-records")
				.about("Migrate committed .monochange release records to the latest schema version")
				.after_help(
					r"Examples:
  monochange migrate release-records --dry-run
  monochange migrate release-records
  monochange migrate release-records --format json

Migration notes:
  - Scans .monochange/releases/*/release.json.
  - Rewrites only records whose schema_version is older than this monochange binary.
  - Use --dry-run to preview which records would change.",
				)
				.arg(
					Arg::new("dry-run")
						.long("dry-run")
						.help("Preview release-record migrations without writing files")
						.action(ArgAction::SetTrue),
				)
				.arg(
					format_arg(MARKDOWN_OUTPUT_FORMATS),
				),
		)
}

pub(crate) fn build_check_subcommand() -> Command {
	Command::new("check")
		.about("Validate configuration, changesets, and run manifest lint rules")
		.after_help(
			"Examples:\n  monochange check\n  monochange check --fix\n  monochange check --ecosystem cargo,npm\n  monochange check --only cargo/sorted-dependencies\n\n\
			 Lint rules are configured in the top-level [lints] section of monochange.toml:\n\n\
			 [lints]\n  use = [\"cargo/recommended\", \"npm/recommended\"]\n\n\
			 [lints.rules]\n  \"cargo/internal-dependency-workspace\" = \"error\"",
		)
		.arg(
			Arg::new("fix")
				.long("fix")
				.short('f')
				.help("Automatically fix lint issues where possible")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Preview the fixes --fix would apply without writing files")
				.action(ArgAction::SetTrue)
				.requires("fix"),
		)
		.arg(
			Arg::new("ecosystem")
				.long("ecosystem")
				.short('e')
				.help("Limit linting to specific lint suites")
				.value_name("ECOSYSTEMS")
				.value_delimiter(','),
		)
		.arg(
			Arg::new("only")
				.long("only")
				.help("Run only the specified lint rule ids")
				.value_name("RULES")
				.value_delimiter(','),
		)
		.arg(
			format_arg(MARKDOWN_OUTPUT_FORMATS),
		)
}

pub(crate) fn build_lint_subcommand() -> Command {
	Command::new("lint")
		.about("Inspect and scaffold manifest lint rules")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("list")
				.about("List registered lint rules and presets")
				.arg(
					format_arg(MARKDOWN_OUTPUT_FORMATS),
		)
		)
		.subcommand(
			Command::new("explain")
				.about("Explain a lint rule or preset")
				.arg(
					Arg::new("id")
						.required(true)
						.help("Lint rule id or preset id to explain"),
				)
				.arg(
					format_arg(MARKDOWN_OUTPUT_FORMATS),
		)
		)
		.subcommand(
			Command::new("new")
				.about("Scaffold a new lint rule in an ecosystem crate")
				.after_help(
					"Examples:\n  monochange lint new cargo/no-path-dependencies\n  monochange lint new npm/require-package-manager",
				)
				.arg(
					Arg::new("id")
						.required(true)
						.help("New lint id in the form <ecosystem>/<rule-name>"),
				),
		)
}

pub(crate) fn build_help_subcommand() -> Command {
	Command::new("help")
		.about("Show detailed help for a command")
		.arg(
			Arg::new("command")
				.help("Built-in command path to get help for (e.g. step validate, init). Config-defined workflow commands are listed too; use `monochange run <name> --help` when a built-in command shares the name")
				.value_name("COMMAND")
				.num_args(0..),
		)
}

/// Arguments for the `completions` command, appended to the built-in set.
pub(crate) fn build_completions_command() -> Command {
	Command::new("completions")
		.about("Generate a shell completion script for monochange")
		.long_about(
			"Print a completion script for the named shell. The script includes the commands \
			 configured in this repository's monochange.toml, so generate it from the repository \
			 you work in.\n\nInstall with your shell's completion mechanism, for example \
			 `monochange completions zsh > \"${fpath[1]}/_monochange\"`.",
		)
		.arg(
			Arg::new("shell")
				.help("Shell to generate completions for")
				.value_name("SHELL")
				.required(true)
				.value_parser(["bash", "elvish", "fish", "powershell", "zsh"]),
		)
}

/// Arguments for the `doctor` command, appended to the built-in set.
pub(crate) fn build_doctor_command() -> Command {
	Command::new("doctor")
		.about("Check the local environment monochange needs and report what is missing")
		.long_about(
			"Run local checks only: git availability, monochange.toml parsing, and the \
			 toolchain of every ecosystem enabled in this workspace. Nothing contacts the \
			 network.",
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format for the report")
				.value_name("FORMAT")
				.default_value("text")
				.value_parser(["text", "json", "json-min", "markdown", "md"]),
		)
}

pub(crate) fn command_supports_release_diff_preview(cli_command: &CliCommandDefinition) -> bool {
	cli_command
		.steps
		.iter()
		.any(|step| matches!(step, CliStepDefinition::PrepareRelease { .. }))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TopLevelStepAlias {
	pub(crate) command: &'static str,
	pub(crate) step: &'static str,
	pub(crate) help_text: &'static str,
	pub(crate) force_dry_run: bool,
	/// Keep the alias working without listing it in help.
	pub(crate) hidden: bool,
}

pub(crate) const TOP_LEVEL_STEP_ALIASES: &[TopLevelStepAlias] = &[
	TopLevelStepAlias {
		command: "create",
		step: "create-change-file",
		help_text: "Create a changeset file for one or more packages",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "discover",
		step: "discover",
		help_text: "Discover packages across supported ecosystems",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "config",
		step: "config",
		help_text: "Render resolved monochange configuration and workspace metadata",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "preview",
		step: "prepare-release",
		help_text: "Preview planned version bumps, changelogs, and release artifacts without writing files",
		force_dry_run: true,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "prepare",
		step: "prepare-release",
		help_text: "Prepare version bumps, changelogs, and release artifacts",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "affected",
		step: "affected-packages",
		help_text: "Evaluate affected packages and changeset coverage",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "diagnose",
		step: "diagnose-changesets",
		help_text: "Inspect changeset provenance and review metadata",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "next",
		step: "display-versions",
		help_text: "Show the next version for each release group and package without writing files",
		force_dry_run: false,
		hidden: false,
	},
	TopLevelStepAlias {
		command: "next-versions",
		step: "display-versions",
		help_text: "Show the next version for each release group and package without writing files",
		force_dry_run: false,
		hidden: true,
	},
];

pub(crate) fn top_level_step_alias(command: &str) -> Option<TopLevelStepAlias> {
	TOP_LEVEL_STEP_ALIASES
		.iter()
		.copied()
		.find(|alias| alias.command == command)
}

pub(crate) fn top_level_step_alias_command_definition(
	alias: TopLevelStepAlias,
) -> Option<CliCommandDefinition> {
	let step = monochange_core::all_step_variants()
		.into_iter()
		.find(|step| step.step_kebab_name() == alias.step)?;

	Some(CliCommandDefinition {
		name: alias.command.to_string(),
		help_text: Some(alias.help_text.to_string()),
		inputs: step.step_inputs_schema(),
		steps: vec![step.with_inherited_step_inputs()],
		dry_run: alias.force_dry_run,
	})
}

fn build_top_level_step_alias_subcommands() -> Vec<Command> {
	TOP_LEVEL_STEP_ALIASES
		.iter()
		.map(|alias| {
			let synthetic = top_level_step_alias_command_definition(*alias)
				.unwrap_or_else(|| panic!("missing step command alias target `{}`", alias.step));
			build_cli_command_subcommand_with_prefix(&synthetic, "monochange").hide(alias.hidden)
		})
		.collect()
}

fn step_command_summary(kebab: &str) -> String {
	match kebab {
		"config" => "Render resolved monochange configuration and workspace metadata",
		"validate" => "Validate configuration, changesets, and version groups",
		"discover" => "Discover packages across supported ecosystems",
		"display-versions" => {
			"Show the next version for each release group and package without writing files"
		}
		"create-change-file" => "Create a changeset file for one or more packages",
		"prepare-release" => "Plan version bumps, changelogs, and release artifacts",
		"commit-release" => "Create a release commit that embeds the durable release record",
		"verify-release-branch" => "Verify a commit is reachable from a configured release branch",
		"publish-release" => "Publish provider release objects from a prepared release artifact",
		"placeholder-publish" => {
			"Publish placeholder package versions for missing registry packages"
		}
		"publish-packages" => "Publish package versions from durable monochange release state",
		"plan-publish-rate-limits" => {
			"Plan package-registry rate-limit windows for publish operations"
		}
		"open-release-request" => {
			"Open or update the hosted release request from prepared release state"
		}
		"comment-released-issues" => "Comment on linked issues shipped in a prepared release",
		"affected-packages" => "Compute affected packages from a prepared release plan",
		"diagnose-changesets" => "Inspect changeset provenance and review metadata",
		"release-record" => "Inspect the release record associated with a tag or commit",
		"publish-readiness" => {
			"Check package registry publishing readiness without publishing packages"
		}
		"tag-release" => "Create and push release tags from a release record",
		"retarget-release" => "Repair a recent release by retargeting its release tags",
		kebab => return format!("Run the built-in {kebab} release workflow step"),
	}
	.to_string()
}

pub(crate) fn build_step_subcommand() -> Command {
	let mut command = Command::new("step")
		.about("Run built-in release workflow steps")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand_help_heading("Step Commands");

	for step in monochange_core::all_step_variants() {
		let step = step.with_inherited_step_inputs();
		let kebab = step.step_kebab_name();
		let synthetic = CliCommandDefinition {
			help_text: Some(step_command_summary(&kebab)),
			name: kebab,
			inputs: step.step_inputs_schema(),
			steps: vec![step],
			dry_run: false,
		};
		let mut subcommand =
			build_cli_command_subcommand_with_prefix(&synthetic, "monochange step");
		if let Some(publish) = PUBLISH_STEP_SUBCOMMANDS
			.iter()
			.find(|publish| publish.step == synthetic.name)
		{
			subcommand = subcommand.after_help(format!(
				"Prefer `monochange publish {}`, which runs this step with the same inputs.",
				publish.command
			));
		}
		command = command.subcommand(subcommand);
	}

	command
}

/// Subcommands grouped under the built-in top-level `publish` command.
///
/// Each entry names the built-in step the subcommand runs. The step supplies
/// the inputs, so `publish packages` accepts exactly what
/// `step publish-packages` accepts.
pub(crate) const PUBLISH_STEP_SUBCOMMANDS: &[PublishStepSubcommand] = &[
	PublishStepSubcommand {
		command: "packages",
		step: "publish-packages",
		help_text: "Publish package versions from durable monochange release state",
	},
	PublishStepSubcommand {
		command: "readiness",
		step: "publish-readiness",
		help_text: "Check package registry publishing readiness without publishing packages",
	},
	PublishStepSubcommand {
		command: "placeholder",
		step: "placeholder-publish",
		help_text: "Publish placeholder package versions for missing registry packages",
	},
];

/// One built-in step exposed as a subcommand of `monochange publish`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PublishStepSubcommand {
	pub(crate) command: &'static str,
	pub(crate) step: &'static str,
	pub(crate) help_text: &'static str,
}

/// Build the step-backed definition for a `monochange publish <command>`
/// subcommand.
///
/// `name` is the command name recorded on the definition. The clap builder
/// passes the short subcommand token so `monochange publish packages` parses,
/// while the runtime passes the fully qualified `publish packages` so progress
/// output and error messages name the command the user typed.
fn publish_step_definition(
	subcommand: PublishStepSubcommand,
	name: String,
) -> CliCommandDefinition {
	let step = monochange_core::all_step_variants()
		.into_iter()
		.find(|step| step.step_kebab_name() == subcommand.step)
		.unwrap_or_else(|| panic!("missing built-in step `{}`", subcommand.step));

	CliCommandDefinition {
		name,
		help_text: Some(subcommand.help_text.to_string()),
		inputs: step.step_inputs_schema(),
		steps: vec![step.with_inherited_step_inputs()],
		dry_run: false,
	}
}

/// Resolve a `monochange publish <command>` invocation into the definition the
/// runtime executes.
///
/// The clap subcommand set is built from [`PUBLISH_STEP_SUBCOMMANDS`], so an
/// unknown name can only arrive through a direct caller such as a test.
pub(crate) fn publish_step_command_definition(
	command: &str,
) -> monochange_core::MonochangeResult<CliCommandDefinition> {
	let subcommand = PUBLISH_STEP_SUBCOMMANDS
		.iter()
		.copied()
		.find(|subcommand| subcommand.command == command)
		.ok_or_else(|| {
			MonochangeError::Config(format!("unknown publish command: publish {command}"))
		})?;

	Ok(publish_step_definition(
		subcommand,
		format!("publish {command}"),
	))
}

fn build_publish_subcommand() -> Command {
	let mut command = Command::new("publish")
		.about("Publish packages and inspect registry publishing state")
		.after_help(
			"Examples:\n  monochange publish readiness --from HEAD --output readiness.json\n  monochange publish packages --output publish.json\n  monochange publish placeholder --format json\n\nPrefer these commands over the matching built-in steps. Each subcommand runs the same step with the same inputs:\n  monochange publish packages     -> monochange step publish-packages\n  monochange publish readiness    -> monochange step publish-readiness\n  monochange publish placeholder  -> monochange step placeholder-publish",
		)
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand_help_heading("Publish Commands");

	for subcommand in PUBLISH_STEP_SUBCOMMANDS {
		let definition = publish_step_definition(*subcommand, subcommand.command.to_string());
		command = command.subcommand(build_cli_command_subcommand_with_prefix(
			&definition,
			"monochange publish",
		));
	}

	command
}

pub(crate) fn build_run_subcommand(cli: &[CliCommandDefinition]) -> Command {
	let mut command = Command::new("run")
		.about("Run workflow commands defined in monochange.toml")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand_help_heading("Workflow Commands");

	for cli_command in cli {
		if cli_command.name == "versions" {
			continue;
		}

		command = command.subcommand(build_cli_command_subcommand_with_prefix(
			cli_command,
			"monochange run",
		));
	}

	command
}

pub(crate) fn build_cli_command_subcommand_with_prefix(
	cli_command: &CliCommandDefinition,
	usage_prefix: &str,
) -> Command {
	let help_text = cli_command
		.help_text
		.clone()
		.unwrap_or_else(|| format!("Run the `{}` command", cli_command.name));

	let mut command = Command::new(leak_string(cli_command.name.clone())).about(help_text);

	for input in &cli_command.inputs {
		command = command.arg(build_cli_command_input_arg(input));
	}

	// `--dry-run` stays accepted everywhere so scripts keep working, but it is
	// only listed for commands with a step that can change something.
	command = command.arg(
		Arg::new("dry-run")
			.long("dry-run")
			.help_heading(RELEASE_OPTIONS_HELP_HEADING)
			.help("Show what would change without writing files or contacting services")
			.action(ArgAction::SetTrue)
			.hide(!cli_command_can_write(cli_command)),
	);

	if command_supports_release_diff_preview(cli_command) {
		command = command
			.arg(
				Arg::new("diff")
					.long("diff")
					.help_heading(RELEASE_OPTIONS_HELP_HEADING)
					.help("Show unified file diffs for prepared release changes")
					.action(ArgAction::SetTrue),
			)
			.arg(
				Arg::new("prepared-release")
					.long("prepared-release")
					.help_heading(RELEASE_OPTIONS_HELP_HEADING)
					.help("Read or write the prepared release artifact at a specific path")
					.value_name("PATH"),
			);
	}

	if let Some(after_help) = cli_command_after_help(cli_command)
		.map(ToString::to_string)
		.or_else(|| cli_command_examples(cli_command, usage_prefix))
	{
		command = command.after_help(after_help);
	}

	command
}

/// Step kinds that only read the workspace, so `--dry-run` changes nothing.
const READ_ONLY_STEP_KINDS: &[&str] = &[
	"Config",
	"Validate",
	"Discover",
	"DisplayVersions",
	"AffectedPackages",
	"DiagnoseChangesets",
	"ReleaseRecord",
	"PublishReadiness",
	"PlanPublishRateLimits",
];

/// Whether any step of `cli_command` can write files or contact a service.
fn cli_command_can_write(cli_command: &CliCommandDefinition) -> bool {
	!cli_command.dry_run
		&& cli_command
			.steps
			.iter()
			.any(|step| !READ_ONLY_STEP_KINDS.contains(&step.kind_name()))
}

/// Examples for built-in step commands that have no dedicated help notes.
fn cli_command_examples(cli_command: &CliCommandDefinition, usage_prefix: &str) -> Option<String> {
	let [step] = cli_command.steps.as_slice() else {
		return None;
	};
	let arguments: &[&str] = match step.kind_name() {
		"Config" | "Discover" | "DiagnoseChangesets" => &["", " --format json"],
		"DisplayVersions" => &["", " --format json --jq '.groups'"],
		"PrepareRelease" => &[" --diff", " --format json"],
		"CreateChangeFile" => {
			&[
				" --interactive",
				" --package core --bump minor --reason \"Add a retry option\"",
			]
		}
		_ => return None,
	};
	let command = format!("{usage_prefix} {}", cli_command.name);
	let mut examples = String::from("Examples:");
	for argument in arguments {
		let _ = write!(examples, "\n  {command}{argument}");
	}
	Some(examples)
}

pub(crate) fn cli_command_after_help(cli_command: &CliCommandDefinition) -> Option<&'static str> {
	match cli_command.name.as_str() {
		"step publish-release" => {
			Some(
				r"What this step does:
  - Reads a prepared release artifact produced by prepare-release.
  - Creates or updates source-control provider release objects, such as GitHub, GitLab, or Gitea releases.
  - Uploads provider release notes and asset metadata according to the configured source provider.

What this step does not do:
  - It does not publish package artifacts to registries such as npm, crates.io, pub.dev, or PyPI.
  - Registry publication is handled by the publish planning and publish commands.

Typical release flow:
  monochange step prepare-release --output prepared-release.json
  monochange step publish-release --prepared-release prepared-release.json

Related commands:
  monochange help step prepare-release
  monochange help publish readiness
  monochange help publish packages",
			)
		}
		"step prepare-release" => {
			Some(
				r"What this step does:
  - Reads pending changesets and workspace package metadata.
  - Calculates version bumps, changelog entries, and release artifacts.
  - Writes a prepared release artifact that later steps can consume.

Typical release flow:
  monochange step prepare-release --output prepared-release.json
  monochange step publish-release --prepared-release prepared-release.json",
			)
		}
		"step affected-packages" => {
			Some(
				r"What this step does:
  - Computes packages affected by a change or release plan.
  - Includes direct changes, grouped packages, and dependent packages according to monochange propagation rules.

Use this command when debugging release scope before preparing or publishing a release.",
			)
		}
		"step create-change-file" => {
			Some(
				r"What this step does:
  - Creates a changeset file for one or more packages.
  - Records bump intent, reason text, and dependency-caused relationships.

Prefer configured package ids in change files whenever a leaf package changed.",
			)
		}
		"change" => {
			Some(
				r#"Examples:
  monochange run change --package sdk-core --bump patch --reason "fix panic"
  monochange run change --package sdk-core --bump minor --reason "add API" --output .changeset/sdk-core.md
  monochange run change --package sdk --bump minor --reason "coordinated release"
  monochange run change --package sdk-config --bump none --caused-by sdk-core --reason "dependency-only follow-up"

Rules:
  - Prefer configured package ids in change files whenever a leaf package changed.
  - Use a group id only when the change is intentionally owned by the whole group.
  - Dependents and grouped members are propagated automatically during planning.
  - Use `--caused-by` when a package is only changing because another package or group moved first.
  - Legacy manifest paths may still resolve during migration, but declared ids are the stable interface."#,
			)
		}
		"release" => {
			Some(
				r"Examples:
  monochange run release --dry-run --format text
  monochange run release --dry-run --format json
  monochange run release --dry-run --diff
  monochange run release

Planning reminders:
  - Direct package changes propagate to dependents using defaults.parent_bump or the target's bump_propagation override (including inherit with an optional bump_propagation_max clamp).
  - Group synchronization happens before final output is rendered.
  - Explicit versions on grouped members propagate to the whole group.",
			)
		}
		"versions" => {
			Some(
				r"Examples:
  monochange versions --dry-run
  monochange versions
  monochange versions --dry-run --format json

Summary notes:
  - This command syncs internal workspace dependency constraints.
  - Use --dry-run to preview manifest edits before writing files.
  - Strategy precedence is package config, ecosystem config, ecosystem default; --strategy overrides.",
			)
		}
		"commit-release" => {
			Some(
				r"Examples:
  monochange run commit-release --dry-run --format json
  monochange run commit-release --dry-run --diff
  monochange run commit-release

Commit notes:
  - Reuses the standard monochange release commit subject/body contract.
  - Embeds a durable release record block in the commit body.
  - Can run before OpenReleaseRequest in the same workflow.",
			)
		}
		"affected" => {
			Some(
				r"Examples:
  monochange affected --changed-paths crates/core/src/lib.rs --format json
  monochange affected --from origin/main --verify

Verification reminders:
  - Prefer package ids in .changeset files.
  - Group-owned changesets cover all members of that group.
  - Ignored paths and skip labels are controlled from [changesets.affected].",
			)
		}
		"diagnostics" => {
			Some(
				r"Examples:
  monochange step diagnose-changesets --format json
  monochange step diagnose-changesets --changeset .changeset/feature.md

Diagnostics include:
  - Target packages/groups and requested bump
  - commit SHA that introduced and last updated each changeset
  - linked review request (when detected)
  - related issue references",
			)
		}
		"repair-release" => {
			Some(
				r"Examples:
  monochange repair-release --from v1.2.3 --dry-run
  monochange repair-release --from v1.2.3 --target HEAD --format json

Repair notes:
  - Finds the release record from history using the supplied ref.
  - Moves the full release tag set together.
  - Defaults to descendant-only retargets unless --force is set.
  - Hosted release sync runs by default and can be disabled with --sync-provider=false.",
			)
		}
		"tag-release" | "step tag-release" => {
			Some(
				r"Examples:
  monochange step tag-release --from HEAD
  monochange step tag-release --from HEAD --dry-run --format json
  monochange step tag-release --from HEAD --push=false

Tagging notes:
  - Requires the resolved ref itself to be the monochange release commit.
  - Creates the full tag set declared by that release record.
  - Treats reruns on the same commit as already up to date.
  - Use `monochange repair-release` if you need to move existing tags later.",
			)
		}
		_ => None,
	}
}

pub(crate) fn cli_command_usage(cli_command: &CliCommandDefinition) -> String {
	cli_command_usage_with_prefix(cli_command, "monochange")
}

pub(crate) fn cli_command_usage_with_prefix(
	cli_command: &CliCommandDefinition,
	usage_prefix: &str,
) -> String {
	let mut usage = String::with_capacity(32 + cli_command.name.len());
	usage.push_str(usage_prefix);
	usage.push(' ');
	usage.push_str(&cli_command.name);
	usage.push_str(" [--dry-run]");

	if command_supports_release_diff_preview(cli_command) {
		usage.push_str(" [--diff] [--prepared-release <PATH>]");
	}

	for input in &cli_command.inputs {
		usage.push(' ');
		usage.push_str(&cli_input_usage(input));
	}

	usage
}

pub(crate) fn cli_input_usage(input: &CliInputDefinition) -> String {
	let long_name = input.name.replace('_', "-");

	let value_name = match input.kind {
		CliInputKind::Boolean => None,
		CliInputKind::Path => Some("PATH".to_string()),
		CliInputKind::String | CliInputKind::StringList | CliInputKind::Choice => {
			Some(input.name.to_uppercase())
		}
	};

	let option = value_name.map_or_else(
		|| format!("--{long_name}"),
		|name| format!("--{long_name} <{name}>"),
	);

	if input.required {
		option
	} else {
		format!("[{option}]")
	}
}

fn build_cli_command_input_arg(input: &CliInputDefinition) -> Arg {
	let long_name = leak_string(input.name.replace('_', "-"));
	let value_name = leak_string(input.name.to_uppercase());
	let help_text = input.help_text.clone().unwrap_or_default();

	let mut arg = Arg::new(leak_string(input.name.clone()))
		.long(long_name)
		.required(input.required)
		.help(help_text);

	arg = match input.kind {
		CliInputKind::String => arg.value_name(value_name),
		CliInputKind::StringList => arg.value_name(value_name).action(ArgAction::Append),
		CliInputKind::Path => arg.value_name("PATH"),

		CliInputKind::Boolean => {
			if input.default.as_deref() == Some("true") {
				arg.value_name(value_name)
					.num_args(0..=1)
					.default_missing_value("true")
					.require_equals(true)
					.value_parser(["true", "false"])
			} else {
				arg.action(ArgAction::SetTrue)
			}
		}

		CliInputKind::Choice => {
			// `--format markdown` is accepted wherever `md` is offered.
			let accepts_markdown = input.choices.iter().any(|choice| choice == "markdown");
			let possible_values: Vec<_> = input
				.choices
				.iter()
				.cloned()
				.map(|choice| {
					let alias = (choice == "md" && !accepts_markdown).then_some("markdown");
					clap::builder::PossibleValue::new(leak_string(choice)).aliases(alias)
				})
				.collect();

			arg.value_name(value_name)
				.value_parser(clap::builder::PossibleValuesParser::new(possible_values))
		}
	};

	if let Some(short) = input.short {
		arg = arg.short(short);
	}

	let should_apply_default = input
		.default
		.as_ref()
		.is_some_and(|default| !matches!(input.kind, CliInputKind::Boolean) || default == "true");

	if should_apply_default {
		arg = arg.default_value(leak_string(input.default.clone().unwrap()));
	}

	arg
}

/// Intentionally leaks a string to obtain a `&'static str` for clap arguments.
///
/// This is acceptable only in CLI binaries where the process lifetime is short
/// and the leaked strings are never freed. Do not use this in library code.
fn leak_string(value: impl Into<String>) -> &'static str {
	Box::leak(value.into().into_boxed_str())
}

pub(crate) fn build_versions_subcommand() -> Command {
	Command::new("versions")
		.about("List package and group versions or sync internal dependency constraints")
		.after_help(
			"Examples:\n  monochange versions list --format json\n  monochange versions sync --dry-run\n  monochange versions sync --dry-run --format json\n  monochange versions sync --strategy exact\n\n\
Calling `monochange versions` without a subcommand checks whether internal dependency constraints \
match package versions without writing files. Run `monochange versions sync` to apply the changes.",
		)
		.arg_required_else_help(false)
		.subcommand_required(false)
		.args(versions_sync_args())
		.subcommand(
			Command::new("list")
				.about("List current package and group versions")
				.after_help("Versions are keyed by the exact monochange package or group id.")
				.arg(
					format_arg(DATA_OUTPUT_FORMATS),
				),
		)
		.subcommand(
			Command::new("sync")
				.about("Sync internal dependency constraints to package versions")
				.after_help(
					"Examples:\n  monochange versions sync --dry-run\n  monochange versions sync --dry-run --format json\n  monochange versions sync --strategy exact\n\n\
Use this when migrating to monochange, checking whether manifests are already in sync, or \
normalizing constraints before grouping packages for shared releases. Strategy precedence is \
package config, then ecosystem config, then the ecosystem default unless --strategy forces one \
style for this run.",
				)
				.args(versions_sync_args()),
		)
}

fn versions_sync_args() -> Vec<Arg> {
	vec![
		Arg::new("dry-run")
			.long("dry-run")
			.help("Show what would change without modifying files")
			.action(ArgAction::SetTrue),
		format_arg(DATA_OUTPUT_FORMATS),
		Arg::new("strategy")
			.long("strategy")
			.value_name("STRATEGY")
			.help(
				"Version constraint style for this run; `default` follows package, then ecosystem config",
			)
			.default_value("default")
			.value_parser(["default", "exact", "caret", "compatible"]),
	]
}

pub(crate) fn current_dir_or_dot() -> PathBuf {
	std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Build classification options from the parsed `change classify`,
/// `api diff`, or `changeset validate --api` arguments.
///
/// # Errors
///
/// Returns an error for unsupported format, detection-level, or
/// dependency-propagation values.
pub fn classify_options_from_matches(
	matches: &clap::ArgMatches,
) -> monochange_core::MonochangeResult<monochange_classification::ClassifyOptions> {
	let format = matches
		.get_one::<String>("format")
		.map_or("text", String::as_str);
	let dependency_propagation = matches
		.get_one::<String>("dependency-propagation")
		.map_or("none", String::as_str);

	Ok(monochange_classification::ClassifyOptions {
		base: matches.get_one::<String>("base").cloned(),
		head: matches
			.get_one::<String>("head")
			.cloned()
			.unwrap_or_else(|| "HEAD".to_string()),
		release: matches.get_one::<String>("release").cloned(),
		packages: matches
			.get_many::<String>("package")
			.into_iter()
			.flatten()
			.cloned()
			.collect(),
		// patch-coverage:ignore-start -- clap restricts this value before extraction; parser success and direct error paths are covered separately, while llvm-cov attributes the propagated `?` to this call site.
		detection_level: monochange_classification::parse_detection_level(
			matches
				.get_one::<String>("detection-level")
				.map_or("signature", String::as_str),
		)?,
		// patch-coverage:ignore-end
		include_unchanged: matches.get_flag("include-unchanged"),
		strict: matches
			.try_get_one::<bool>("strict")
			.ok()
			.flatten()
			.copied()
			.unwrap_or(false),
		skip_cli_snapshots: monochange_classification::skip_cli_snapshots_for(
			matches.get_flag("skip-cli-snapshots"),
			std::env::var(monochange_classification::SKIP_CLI_SNAPSHOTS_ENV)
				.ok()
				.as_deref(),
		),
		format: monochange_classification::ClassificationFormat::parse(format)
			.map_err(MonochangeError::Config)?,
		output: matches.get_one::<String>("output").map(PathBuf::from),
		// Only the classification-producing subcommands declare `--label`;
		// `changeset validate` shares this parser without the skip policy.
		labels: matches
			.try_get_many::<String>("label")
			.ok()
			.flatten()
			.into_iter()
			.flatten()
			.cloned()
			.collect(),
		dependency_propagation: monochange_classification::parse_dependency_propagation(
			dependency_propagation,
		)?,
	})
}

#[cfg(test)]
#[path = "__tests__/cli_tests.rs"]
mod tests;
