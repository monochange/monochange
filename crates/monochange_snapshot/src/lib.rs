//! Normalized snapshots for command and API surfaces.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use clap::Arg;
use clap::ArgAction;
use clap::Command;
use serde::Deserialize;
use serde::Serialize;

/// Snapshot schema version emitted by this crate.
///
/// The value is derived from the `monochange_snapshot` crate version by dropping
/// the patch component. For example, crate version `0.7.0` emits snapshot schema
/// version `0.7`.
pub const SNAPSHOT_SCHEMA_VERSION: &str = env!("MONOCHANGE_SNAPSHOT_SCHEMA_VERSION");

/// A framework-neutral command surface snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSnapshot {
	pub schema_version: String,
	pub kind: SnapshotKind,
	pub tool: SnapshotTool,
	pub provenance: SnapshotProvenance,
	pub standard_entrypoints: StandardEntrypoints,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub global_options: Vec<CommandOption>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub commands: Vec<CommandNode>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub output_contracts: Vec<OutputContract>,
}

/// Snapshot kind discriminator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotKind {
	CliSurface,
}

/// Tool identity captured in a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotTool {
	pub name: String,
	pub version: Option<String>,
}

/// Snapshot extraction provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotProvenance {
	pub extractor: String,
	pub confidence: SnapshotConfidence,
}

/// Extractor confidence level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotConfidence {
	High,
	Medium,
	Low,
}

/// Standard CLI entrypoints normalized across spelling variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardEntrypoints {
	pub help: StandardEntrypoint,
	pub version: StandardEntrypoint,
	pub snapshot: StandardEntrypoint,
}

/// Standard entrypoint spellings supported by a tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StandardEntrypoint {
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub commands: Vec<Vec<String>>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub flags: Vec<String>,
}

/// One command in a command surface snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandNode {
	pub path: Vec<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub aliases: Vec<String>,
	pub hidden: bool,
	pub stability: Stability,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub summary: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub parser: ParserBehavior,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub options: Vec<CommandOption>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub positionals: Vec<CommandPositional>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub commands: Vec<CommandNode>,
}

/// Stability label for a command or output contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stability {
	Stable,
	Experimental,
	HumanReadable,
}

/// Parser behavior that affects invocation compatibility.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParserBehavior {
	pub flags_are_posix_noncompliant: bool,
	pub options_must_precede_arguments: bool,
	pub option_arg_separators: Vec<String>,
}

/// A named command option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOption {
	pub names: Vec<String>,
	pub canonical_name: String,
	pub hidden: bool,
	pub global: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub summary: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub value: OptionValue,
}

/// A positional command argument.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPositional {
	pub name: String,
	pub hidden: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub summary: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub value: OptionValue,
}

/// Accepted value shape for an option or positional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionValue {
	pub kind: ValueKind,
	pub required: bool,
	pub repeatable: bool,
	pub variadic: bool,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub enum_values: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub default: Option<String>,
}

/// CLI token value kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValueKind {
	Flag,
	String,
	Enum,
	Counter,
}

/// Stable output contract annotation for a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputContract {
	pub command_path: Vec<String>,
	pub stream: OutputStream,
	pub format: OutputFormat,
	pub stability: Stability,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub exit_codes: Vec<i32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub schema_ref: Option<String>,
}

/// Output stream covered by a command output contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputStream {
	Stdout,
	Stderr,
}

/// Machine-readable output format covered by a command output contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputFormat {
	Json,
	Text,
}

/// Framework-neutral CLI surface extractor.
pub trait CommandSurfaceExtractor {
	/// Human-readable extractor name for provenance.
	fn name(&self) -> &'static str;

	/// Extract a normalized command snapshot.
	fn extract(&self) -> CommandSnapshot;
}

/// Clap-backed command surface extractor.
pub struct ClapCommandSurfaceExtractor<'command> {
	command: &'command Command,
}

impl<'command> ClapCommandSurfaceExtractor<'command> {
	/// Create a clap extractor for a command definition.
	#[must_use]
	pub fn new(command: &'command Command) -> Self {
		Self { command }
	}
}

impl CommandSurfaceExtractor for ClapCommandSurfaceExtractor<'_> {
	fn name(&self) -> &'static str {
		"clap"
	}

	fn extract(&self) -> CommandSnapshot {
		snapshot_from_clap(self.command)
	}
}

/// Snapshot render mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotView {
	Full,
	Light,
	Index,
}

impl CommandSnapshot {
	/// Render this snapshot as pretty JSON.
	pub fn to_json(&self) -> Result<String, serde_json::Error> {
		serde_json::to_string_pretty(self).map(|mut json| {
			json.push('\n');
			json
		})
	}

	/// Return a view optimized for the requested consumer.
	#[must_use]
	pub fn view(&self, view: SnapshotView) -> Self {
		match view {
			SnapshotView::Full => self.clone(),
			SnapshotView::Light => self.without_descriptions(),
			SnapshotView::Index => self.as_index(),
		}
	}

	/// Return the snapshot for a command subtree.
	#[must_use]
	pub fn subtree(&self, path: &[String]) -> Option<Self> {
		if path.is_empty() {
			return Some(self.clone());
		}

		let command = find_command(&self.commands, path)?;
		let mut snapshot = self.clone();
		snapshot.commands = vec![command.clone()];
		Some(snapshot)
	}

	fn without_descriptions(&self) -> Self {
		let mut snapshot = self.clone();
		for option in &mut snapshot.global_options {
			option.summary = None;
			option.description = None;
		}
		strip_command_descriptions(&mut snapshot.commands);
		snapshot
	}

	fn as_index(&self) -> Self {
		let mut snapshot = self.without_descriptions();
		snapshot.global_options.clear();
		for command in &mut snapshot.commands {
			strip_command_details(command);
		}
		snapshot
	}
}

/// Build a high-confidence snapshot from clap command metadata.
#[must_use]
pub fn snapshot_from_clap(command: &Command) -> CommandSnapshot {
	let mut root = command.clone();
	let name = root.get_name().to_string();
	let version = root.get_version().map(str::to_string);
	let standard_entrypoints = standard_entrypoints(&root);
	let global_options = root
		.get_arguments()
		.filter(|arg| arg.is_global_set())
		.filter_map(command_option_from_arg)
		.collect();
	let commands = root
		.get_subcommands_mut()
		.filter(|subcommand| !is_builtin_metadata_subcommand(subcommand.get_name()))
		.map(|subcommand| command_node_from_clap(subcommand, Vec::new()))
		.collect();

	CommandSnapshot {
		schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
		kind: SnapshotKind::CliSurface,
		tool: SnapshotTool { name, version },
		provenance: SnapshotProvenance {
			extractor: "clap".to_string(),
			confidence: SnapshotConfidence::High,
		},
		standard_entrypoints,
		global_options,
		commands,
		output_contracts: Vec::new(),
	}
}

fn command_node_from_clap(command: &mut Command, parent_path: Vec<String>) -> CommandNode {
	let mut path = parent_path;
	path.push(command.get_name().to_string());
	let parser = ParserBehavior {
		flags_are_posix_noncompliant: false,
		options_must_precede_arguments: false,
		option_arg_separators: vec![" ".to_string(), "=".to_string()],
	};
	let options = command
		.get_arguments()
		.filter(|arg| !arg.is_positional())
		.filter_map(command_option_from_arg)
		.collect();
	let positionals = command
		.get_arguments()
		.filter(|arg| arg.is_positional())
		.map(command_positional_from_arg)
		.collect();
	let commands = command
		.get_subcommands_mut()
		.filter(|subcommand| !is_builtin_metadata_subcommand(subcommand.get_name()))
		.map(|subcommand| command_node_from_clap(subcommand, path.clone()))
		.collect();

	CommandNode {
		path,
		aliases: visible_aliases(command).collect(),
		hidden: command.is_hide_set(),
		stability: Stability::Stable,
		summary: styled_str(command.get_about()),
		description: styled_str(command.get_long_about()),
		parser,
		options,
		positionals,
		commands,
	}
}

fn command_option_from_arg(arg: &Arg) -> Option<CommandOption> {
	let names = option_names(arg);
	let canonical_name = names.first()?.clone();

	Some(CommandOption {
		names,
		canonical_name,
		hidden: arg.is_hide_set(),
		global: arg.is_global_set(),
		summary: styled_str(arg.get_help()),
		description: styled_str(arg.get_long_help()),
		value: option_value_from_arg(arg),
	})
}

fn command_positional_from_arg(arg: &Arg) -> CommandPositional {
	CommandPositional {
		name: arg.get_id().to_string(),
		hidden: arg.is_hide_set(),
		summary: styled_str(arg.get_help()),
		description: styled_str(arg.get_long_help()),
		value: option_value_from_arg(arg),
	}
}

fn option_value_from_arg(arg: &Arg) -> OptionValue {
	let enum_values = enum_values(arg);
	let kind = if is_flag(arg) {
		ValueKind::Flag
	} else if !enum_values.is_empty() {
		ValueKind::Enum
	} else if matches!(arg.get_action(), ArgAction::Count) {
		ValueKind::Counter
	} else {
		ValueKind::String
	};

	OptionValue {
		kind,
		required: arg.is_required_set(),
		repeatable: arg.get_action().takes_values()
			&& arg
				.get_num_args()
				.is_none_or(|num_args| num_args.max_values() != 1),
		variadic: arg.is_trailing_var_arg_set()
			|| arg
				.get_num_args()
				.is_some_and(|num_args| num_args.max_values() > 1),
		enum_values,
		default: arg
			.get_default_values()
			.first()
			.and_then(|value| value.to_str())
			.map(ToString::to_string),
	}
}

fn option_names(arg: &Arg) -> Vec<String> {
	let mut names = Vec::new();
	if let Some(long) = arg.get_long() {
		names.push(format!("--{long}"));
	}
	if let Some(short) = arg.get_short() {
		names.push(format!("-{short}"));
	}
	if names.is_empty() {
		names.push(arg.get_id().to_string());
	}
	for alias in arg.get_visible_aliases().unwrap_or_default() {
		names.push(format!("--{alias}"));
	}
	for short_alias in arg.get_visible_short_aliases().unwrap_or_default() {
		names.push(format!("-{short_alias}"));
	}
	unique(names)
}

fn enum_values(arg: &Arg) -> Vec<String> {
	arg.get_value_parser()
		.possible_values()
		.map(|values| values.map(|value| value.get_name().to_string()).collect())
		.unwrap_or_default()
}

fn is_flag(arg: &Arg) -> bool {
	matches!(
		arg.get_action(),
		ArgAction::SetTrue | ArgAction::SetFalse | ArgAction::Help | ArgAction::Version
	)
}

fn visible_aliases(command: &Command) -> impl Iterator<Item = String> + '_ {
	command.get_visible_aliases().map(ToString::to_string)
}

fn standard_entrypoints(command: &Command) -> StandardEntrypoints {
	let help_flags = command
		.get_arguments()
		.find(|arg| matches!(arg.get_action(), ArgAction::Help))
		.map_or_else(
			|| vec!["--help".to_string(), "-h".to_string()],
			option_names,
		);
	let version_flags = command
		.get_arguments()
		.find(|arg| matches!(arg.get_action(), ArgAction::Version))
		.map_or_else(
			|| vec!["--version".to_string(), "-V".to_string()],
			option_names,
		);

	StandardEntrypoints {
		help: StandardEntrypoint {
			commands: vec![vec!["help".to_string()]],
			flags: help_flags,
		},
		version: StandardEntrypoint {
			commands: vec![vec!["version".to_string()]],
			flags: version_flags,
		},
		snapshot: StandardEntrypoint {
			commands: vec![vec!["snapshot".to_string()]],
			flags: vec!["--snapshot".to_string()],
		},
	}
}

fn styled_str(value: Option<&clap::builder::StyledStr>) -> Option<String> {
	value
		.map(ToString::to_string)
		.filter(|value| !value.is_empty())
}

fn find_command<'a>(commands: &'a [CommandNode], path: &[String]) -> Option<&'a CommandNode> {
	let (first, rest) = path.split_first()?;
	let command = commands
		.iter()
		.find(|command| command.path.last() == Some(first))?;
	if rest.is_empty() {
		return Some(command);
	}
	find_command(&command.commands, rest)
}

fn strip_command_descriptions(commands: &mut [CommandNode]) {
	for command in commands {
		command.summary = None;
		command.description = None;
		for option in &mut command.options {
			option.summary = None;
			option.description = None;
		}
		for positional in &mut command.positionals {
			positional.summary = None;
			positional.description = None;
		}
		strip_command_descriptions(&mut command.commands);
	}
}

fn strip_command_details(command: &mut CommandNode) {
	command.options.clear();
	command.positionals.clear();
	for child in &mut command.commands {
		strip_command_details(child);
	}
}

fn is_builtin_metadata_subcommand(name: &str) -> bool {
	matches!(name, "help" | "snapshot")
}

fn unique(values: Vec<String>) -> Vec<String> {
	let mut seen = BTreeSet::new();
	let mut unique_values = Vec::new();
	for value in values {
		if seen.insert(value.clone()) {
			unique_values.push(value);
		}
	}
	unique_values
}

/// Severity recommendation for snapshot changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotSeverity {
	None,
	Patch,
	Minor,
	Major,
}

/// Compatibility classification for two command snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotDiffReport {
	pub recommendation: SnapshotSeverity,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub changes: Vec<SnapshotChange>,
}

/// One classified snapshot change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChange {
	pub severity: SnapshotSeverity,
	pub path: Vec<String>,
	pub kind: SnapshotChangeKind,
	pub summary: String,
}

/// Snapshot change kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotChangeKind {
	CommandAdded,
	CommandRemoved,
	CommandDescriptionChanged,
	OptionAdded,
	OptionRemoved,
	OptionDescriptionChanged,
	OptionValueWidened,
	OptionValueNarrowed,
	PositionalAdded,
	PositionalRemoved,
	PositionalChanged,
}

/// Classify compatibility impact between two command snapshots.
#[must_use]
pub fn diff_command_snapshots(
	before: &CommandSnapshot,
	after: &CommandSnapshot,
) -> SnapshotDiffReport {
	let mut changes = Vec::new();
	let before_commands = flatten_commands(&before.commands);
	let after_commands = flatten_commands(&after.commands);

	for (path, command) in &before_commands {
		let Some(after_command) = after_commands.get(path) else {
			changes.push(snapshot_change(
				SnapshotSeverity::Major,
				path,
				SnapshotChangeKind::CommandRemoved,
				format!("command `{}` was removed", path.join(" ")),
			));
			continue;
		};
		classify_command_changes(command, after_command, &mut changes);
	}

	for path in after_commands.keys() {
		if !before_commands.contains_key(path) {
			changes.push(snapshot_change(
				SnapshotSeverity::Minor,
				path,
				SnapshotChangeKind::CommandAdded,
				format!("command `{}` was added", path.join(" ")),
			));
		}
	}

	let recommendation = changes
		.iter()
		.map(|change| change.severity)
		.max()
		.unwrap_or(SnapshotSeverity::None);
	SnapshotDiffReport {
		recommendation,
		changes,
	}
}

fn classify_command_changes(
	before: &CommandNode,
	after: &CommandNode,
	changes: &mut Vec<SnapshotChange>,
) {
	if before.summary != after.summary || before.description != after.description {
		changes.push(snapshot_change(
			SnapshotSeverity::Patch,
			&before.path,
			SnapshotChangeKind::CommandDescriptionChanged,
			format!("command `{}` description changed", before.path.join(" ")),
		));
	}
	classify_option_changes(before, after, changes);
	classify_positional_changes(before, after, changes);
}

fn classify_option_changes(
	before: &CommandNode,
	after: &CommandNode,
	changes: &mut Vec<SnapshotChange>,
) {
	let before_options = options_by_canonical_name(&before.options);
	let after_options = options_by_canonical_name(&after.options);
	for (name, option) in &before_options {
		let Some(after_option) = after_options.get(name) else {
			changes.push(snapshot_change(
				SnapshotSeverity::Major,
				&before.path,
				SnapshotChangeKind::OptionRemoved,
				format!(
					"option `{name}` was removed from `{}`",
					before.path.join(" ")
				),
			));
			continue;
		};
		classify_single_option_change(&before.path, option, after_option, changes);
	}
	for name in after_options.keys() {
		if !before_options.contains_key(name) {
			changes.push(snapshot_change(
				SnapshotSeverity::Minor,
				&before.path,
				SnapshotChangeKind::OptionAdded,
				format!("option `{name}` was added to `{}`", before.path.join(" ")),
			));
		}
	}
}

fn classify_single_option_change(
	path: &[String],
	before: &CommandOption,
	after: &CommandOption,
	changes: &mut Vec<SnapshotChange>,
) {
	if before.summary != after.summary || before.description != after.description {
		changes.push(snapshot_change(
			SnapshotSeverity::Patch,
			path,
			SnapshotChangeKind::OptionDescriptionChanged,
			format!("option `{}` description changed", before.canonical_name),
		));
	}
	let severity = classify_value_change(&before.value, &after.value);
	if severity == SnapshotSeverity::Major {
		changes.push(snapshot_change(
			SnapshotSeverity::Major,
			path,
			SnapshotChangeKind::OptionValueNarrowed,
			format!(
				"option `{}` accepted value contract narrowed",
				before.canonical_name
			),
		));
	} else if severity == SnapshotSeverity::Minor {
		changes.push(snapshot_change(
			SnapshotSeverity::Minor,
			path,
			SnapshotChangeKind::OptionValueWidened,
			format!(
				"option `{}` accepted value contract widened",
				before.canonical_name
			),
		));
	}
}

fn classify_positional_changes(
	before: &CommandNode,
	after: &CommandNode,
	changes: &mut Vec<SnapshotChange>,
) {
	let max_len = before.positionals.len().max(after.positionals.len());
	for index in 0..max_len {
		match (before.positionals.get(index), after.positionals.get(index)) {
			(Some(before), Some(after)) => {
				if before.name != after.name
					|| classify_value_change(&before.value, &after.value) == SnapshotSeverity::Major
				{
					changes.push(snapshot_change(
						SnapshotSeverity::Major,
						&before
							.name
							.split('\0')
							.map(ToString::to_string)
							.collect::<Vec<_>>(),
						SnapshotChangeKind::PositionalChanged,
						format!("positional `{}` changed", before.name),
					));
				}
			}
			(Some(removed), None) => {
				changes.push(snapshot_change(
					SnapshotSeverity::Major,
					&before.path,
					SnapshotChangeKind::PositionalRemoved,
					format!("positional `{}` was removed", removed.name),
				));
			}
			(None, Some(added)) => {
				changes.push(snapshot_change(
					if added.value.required {
						SnapshotSeverity::Major
					} else {
						SnapshotSeverity::Minor
					},
					&before.path,
					SnapshotChangeKind::PositionalAdded,
					format!("positional `{}` was added", added.name),
				));
			}
			// patch-coverage:ignore-start -- range upper bound makes this defensive arm unreachable.
			(None, None) => {} // patch-coverage:ignore-end
		}
	}
}

fn classify_value_change(before: &OptionValue, after: &OptionValue) -> SnapshotSeverity {
	if !before.required && after.required {
		return SnapshotSeverity::Major;
	}
	if before.required && !after.required {
		return SnapshotSeverity::Minor;
	}
	if before.repeatable && !after.repeatable || before.variadic && !after.variadic {
		return SnapshotSeverity::Major;
	}
	if !before.repeatable && after.repeatable || !before.variadic && after.variadic {
		return SnapshotSeverity::Minor;
	}
	let before_values = before.enum_values.iter().collect::<BTreeSet<_>>();
	let after_values = after.enum_values.iter().collect::<BTreeSet<_>>();
	if !before_values.is_empty() && !before_values.is_subset(&after_values) {
		return SnapshotSeverity::Major;
	}
	if !after_values.is_empty() && !after_values.is_subset(&before_values) {
		return SnapshotSeverity::Minor;
	}
	match (before.kind, after.kind) {
		(ValueKind::Enum, ValueKind::String) => SnapshotSeverity::Minor,
		(ValueKind::String, ValueKind::Enum) => SnapshotSeverity::Major,
		(before, after) if before != after => SnapshotSeverity::Major,
		_ => SnapshotSeverity::None,
	}
}

fn flatten_commands(commands: &[CommandNode]) -> BTreeMap<Vec<String>, &CommandNode> {
	let mut flattened = BTreeMap::new();
	for command in commands {
		flattened.insert(command.path.clone(), command);
		flattened.extend(flatten_commands(&command.commands));
	}
	flattened
}

fn options_by_canonical_name(options: &[CommandOption]) -> BTreeMap<&str, &CommandOption> {
	options
		.iter()
		.map(|option| (option.canonical_name.as_str(), option))
		.collect()
}

fn snapshot_change(
	severity: SnapshotSeverity,
	path: &[String],
	kind: SnapshotChangeKind,
	summary: String,
) -> SnapshotChange {
	SnapshotChange {
		severity,
		path: path.to_vec(),
		kind,
		summary,
	}
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
