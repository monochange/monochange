use clap::Arg;
use clap::ArgAction;
use clap::Command;

use super::*;

#[test]
fn option_names_falls_back_to_arg_id_without_visible_names() {
	let names = option_names(&Arg::new("plain").action(ArgAction::SetTrue));
	assert_eq!(names, vec!["plain".to_string()]);
}

#[test]
fn snapshot_schema_version_tracks_package_major_minor() {
	let package_version = env!("CARGO_PKG_VERSION");
	let mut components = package_version.split('.');
	let major = components
		.next()
		.unwrap_or_else(|| panic!("major component"));
	let minor = components
		.next()
		.unwrap_or_else(|| panic!("minor component"));
	let expected = if major == "0" && minor == "0" {
		"0.1".to_string()
	} else {
		format!("{major}.{minor}")
	};
	assert_eq!(SNAPSHOT_SCHEMA_VERSION, expected);
}

#[test]
fn snapshot_from_clap_captures_command_options_and_positionals() {
	let command = Command::new("demo")
		.version("1.2.3")
		.about("Demo CLI")
		.arg(
			Arg::new("verbose")
				.long("verbose")
				.short('v')
				.global(true)
				.help("Increase verbosity")
				.action(ArgAction::SetTrue),
		)
		.subcommand(
			Command::new("run")
				.about("Run a task")
				.arg(
					Arg::new("format")
						.long("format")
						.value_parser(["json", "text"])
						.default_value("text")
						.help("Output format"),
				)
				.arg(Arg::new("task").required(true).help("Task name")),
		);

	let snapshot = snapshot_from_clap(&command);

	assert_eq!(snapshot.tool.name, "demo");
	assert_eq!(snapshot.tool.version.as_deref(), Some("1.2.3"));
	assert_eq!(snapshot.global_options[0].names, vec!["--verbose", "-v"]);
	let run = &snapshot.commands[0];
	assert_eq!(run.path, vec!["run"]);
	assert_eq!(run.summary.as_deref(), Some("Run a task"));
	assert_eq!(run.options[0].value.kind, ValueKind::Enum);
	assert_eq!(run.options[0].value.enum_values, vec!["json", "text"]);
	assert_eq!(run.positionals[0].name, "task");
	assert!(run.positionals[0].value.required);
}

#[test]
fn subtree_returns_requested_command_surface() {
	let command = Command::new("demo")
		.subcommand(Command::new("alpha").subcommand(Command::new("beta")))
		.subcommand(Command::new("gamma"));
	let snapshot = snapshot_from_clap(&command);
	let subtree = snapshot
		.subtree(&["alpha".to_string(), "beta".to_string()])
		.unwrap_or_else(|| panic!("expected subtree"));

	assert_eq!(subtree.commands.len(), 1);
	assert_eq!(subtree.commands[0].path, vec!["alpha", "beta"]);
}

#[test]
fn light_view_omits_descriptive_text() {
	let command = Command::new("demo")
		.about("Demo CLI")
		.subcommand(Command::new("run").about("Run a task"));
	let snapshot = snapshot_from_clap(&command);
	let light = snapshot.view(SnapshotView::Light);

	assert_eq!(light.commands[0].summary, None);
}

#[test]
fn diff_classifies_removed_command_as_major() {
	let before = snapshot_from_clap(&Command::new("demo").subcommand(Command::new("run")));
	let after = snapshot_from_clap(&Command::new("demo"));
	let report = diff_command_snapshots(&before, &after);

	assert_eq!(report.recommendation, SnapshotSeverity::Major);
	assert_eq!(report.changes[0].kind, SnapshotChangeKind::CommandRemoved);
}

#[test]
fn diff_caps_changes_by_nearest_max_semver_bump() {
	let mut before_parent = command_node("experimental");
	before_parent.max_semver_bump = SnapshotSeverity::Minor;
	let mut before_child = command_node("child");
	before_child.path = vec!["experimental".to_string(), "child".to_string()];
	before_parent.commands.push(before_child);
	let before = command_snapshot(vec![before_parent]);

	let mut after_parent = command_node("experimental");
	after_parent.max_semver_bump = SnapshotSeverity::Minor;
	let after = command_snapshot(vec![after_parent]);

	let report = diff_command_snapshots(&before, &after);
	assert_eq!(report.recommendation, SnapshotSeverity::Minor);
	assert_eq!(report.changes[0].severity, SnapshotSeverity::Minor);
}

#[test]
fn diff_uses_stricter_max_semver_bump_from_either_snapshot() {
	let mut before_command = command_node("experimental");
	before_command.max_semver_bump = SnapshotSeverity::None;
	let mut after_command = command_node("experimental");
	after_command.max_semver_bump = SnapshotSeverity::Major;
	after_command
		.options
		.push(option("--new", value(ValueKind::Flag)));

	let report = diff_command_snapshots(
		&command_snapshot(vec![before_command]),
		&command_snapshot(vec![after_command]),
	);
	assert_eq!(report.recommendation, SnapshotSeverity::None);
	assert_eq!(report.changes[0].severity, SnapshotSeverity::None);
}

#[test]
fn command_node_defaults_max_semver_bump_to_major_when_missing() {
	let json = r#"
		{
			"path": ["run"],
			"hidden": false,
			"parser": {
				"flags_are_posix_noncompliant": false,
				"options_must_precede_arguments": false,
				"option_arg_separators": [" ", "="]
			}
		}
	"#;
	let command: CommandNode = serde_json::from_str(json)
		.unwrap_or_else(|error| panic!("deserialize command node: {error}"));
	assert_eq!(command.max_semver_bump, SnapshotSeverity::Major);
}

#[test]
fn diff_classifies_optional_option_addition_as_minor() {
	let before = snapshot_from_clap(&Command::new("demo").subcommand(Command::new("run")));
	let after = snapshot_from_clap(
		&Command::new("demo")
			.subcommand(Command::new("run").arg(Arg::new("format").long("format"))),
	);
	let report = diff_command_snapshots(&before, &after);

	assert_eq!(report.recommendation, SnapshotSeverity::Minor);
	assert_eq!(report.changes[0].kind, SnapshotChangeKind::OptionAdded);
}

#[test]
fn diff_classifies_description_only_change_as_patch() {
	let before =
		snapshot_from_clap(&Command::new("demo").subcommand(Command::new("run").about("Run")));
	let after = snapshot_from_clap(
		&Command::new("demo").subcommand(Command::new("run").about("Run a task")),
	);
	let report = diff_command_snapshots(&before, &after);

	assert_eq!(report.recommendation, SnapshotSeverity::Patch);
	assert_eq!(
		report.changes[0].kind,
		SnapshotChangeKind::CommandDescriptionChanged
	);
}

fn command_snapshot(commands: Vec<CommandNode>) -> CommandSnapshot {
	CommandSnapshot {
		schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
		kind: SnapshotKind::CliSurface,
		tool: SnapshotTool {
			name: "demo".to_string(),
			version: None,
		},
		provenance: SnapshotProvenance {
			extractor: "test".to_string(),
			confidence: SnapshotConfidence::High,
		},
		standard_entrypoints: StandardEntrypoints {
			help: StandardEntrypoint::default(),
			version: StandardEntrypoint::default(),
			snapshot: StandardEntrypoint::default(),
		},
		global_options: Vec::new(),
		commands,
		output_contracts: Vec::new(),
	}
}

fn command_node(name: &str) -> CommandNode {
	CommandNode {
		path: vec![name.to_string()],
		aliases: Vec::new(),
		hidden: false,
		max_semver_bump: SnapshotSeverity::Major,
		summary: None,
		description: None,
		parser: ParserBehavior {
			flags_are_posix_noncompliant: false,
			options_must_precede_arguments: false,
			option_arg_separators: vec![" ".to_string(), "=".to_string()],
		},
		options: Vec::new(),
		positionals: Vec::new(),
		commands: Vec::new(),
	}
}

fn option(name: &str, value: OptionValue) -> CommandOption {
	CommandOption {
		names: vec![name.to_string()],
		canonical_name: name.to_string(),
		hidden: false,
		global: false,
		summary: None,
		description: None,
		value,
	}
}

fn positional(name: &str, value: OptionValue) -> CommandPositional {
	CommandPositional {
		name: name.to_string(),
		hidden: false,
		summary: None,
		description: None,
		value,
	}
}

fn value(kind: ValueKind) -> OptionValue {
	OptionValue {
		kind,
		required: false,
		repeatable: false,
		variadic: false,
		enum_values: Vec::new(),
		default: None,
	}
}

#[test]
fn clap_extractor_trait_and_json_render_are_usable() {
	let command = Command::new("demo")
		.arg(
			Arg::new("debug")
				.long("debug")
				.visible_alias("debug-alias")
				.visible_short_alias('d')
				.action(ArgAction::SetFalse)
				.global(true),
		)
		.arg(
			Arg::new("verbose")
				.short('v')
				.action(ArgAction::Count)
				.global(true),
		)
		.arg(
			Arg::new("plain")
				.alias("plain-alias")
				.short_alias('p')
				.action(ArgAction::SetTrue),
		)
		.subcommand(Command::new("run"));
	let extractor = ClapCommandSurfaceExtractor::new(&command);
	let snapshot = extractor.extract();

	assert_eq!(extractor.name(), "clap");
	assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
	assert!(
		snapshot
			.to_json()
			.unwrap_or_else(|error| panic!("json: {error}"))
			.ends_with('\n')
	);
	assert!(snapshot.subtree(&[]).is_some());
	assert!(snapshot.subtree(&["missing".to_string()]).is_none());
	assert!(matches!(
		snapshot.view(SnapshotView::Full).kind,
		SnapshotKind::CliSurface
	));
	assert!(snapshot.view(SnapshotView::Index).global_options.is_empty());
	assert_eq!(snapshot.global_options[0].value.kind, ValueKind::Flag);
	assert_eq!(snapshot.global_options[1].value.kind, ValueKind::Counter);
	let option_names = snapshot
		.global_options
		.iter()
		.chain(
			snapshot
				.commands
				.iter()
				.flat_map(|command| command.options.iter()),
		)
		.flat_map(|option| option.names.iter())
		.cloned()
		.collect::<Vec<_>>();
	assert!(option_names.contains(&"--debug".to_string()));
	assert!(option_names.contains(&"--debug-alias".to_string()));
	assert!(option_names.contains(&"-d".to_string()));
}

#[test]
fn diff_classifies_added_positionals_by_requiredness() {
	let before = command_snapshot(vec![command_node("demo")]);
	let mut after = command_snapshot(vec![command_node("demo")]);
	after.commands[0].positionals.push(positional(
		"required",
		OptionValue {
			required: true,
			..value(ValueKind::String)
		},
	));
	let report = diff_command_snapshots(&before, &after);
	assert_eq!(report.changes[0].severity, SnapshotSeverity::Major);
	assert_eq!(report.changes[0].kind, SnapshotChangeKind::PositionalAdded);

	let mut after = command_snapshot(vec![command_node("demo")]);
	after.commands[0]
		.positionals
		.push(positional("optional", value(ValueKind::String)));
	let report = diff_command_snapshots(&before, &after);
	assert_eq!(report.changes[0].severity, SnapshotSeverity::Minor);
	assert_eq!(report.changes[0].kind, SnapshotChangeKind::PositionalAdded);
}

#[test]
fn diff_classifies_added_command_as_minor() {
	let before = command_snapshot(Vec::new());
	let after = command_snapshot(vec![command_node("run")]);
	let report = diff_command_snapshots(&before, &after);

	assert_eq!(report.recommendation, SnapshotSeverity::Minor);
	assert_eq!(report.changes[0].kind, SnapshotChangeKind::CommandAdded);
}

#[test]
fn diff_classifies_option_removal_and_description_changes() {
	let mut before_command = command_node("run");
	let mut after_command = command_node("run");
	before_command
		.options
		.push(option("--format", value(ValueKind::String)));
	let mut before_verbose = option("--verbose", value(ValueKind::Flag));
	before_verbose.summary = Some("old".to_string());
	let mut after_verbose = option("--verbose", value(ValueKind::Flag));
	after_verbose.summary = Some("new".to_string());
	before_command.options.push(before_verbose);
	after_command.options.push(after_verbose);
	let report = diff_command_snapshots(
		&command_snapshot(vec![before_command]),
		&command_snapshot(vec![after_command]),
	);

	assert_eq!(report.recommendation, SnapshotSeverity::Major);
	assert!(
		report
			.changes
			.iter()
			.any(|change| change.kind == SnapshotChangeKind::OptionRemoved)
	);
	assert!(
		report
			.changes
			.iter()
			.any(|change| change.kind == SnapshotChangeKind::OptionDescriptionChanged)
	);
}

#[test]
fn diff_classifies_value_contract_narrowing_and_widening() {
	let cases = [
		(
			value(ValueKind::String),
			OptionValue {
				required: true,
				..value(ValueKind::String)
			},
			SnapshotSeverity::Major,
		),
		(
			OptionValue {
				required: true,
				..value(ValueKind::String)
			},
			value(ValueKind::String),
			SnapshotSeverity::Minor,
		),
		(
			OptionValue {
				repeatable: true,
				..value(ValueKind::String)
			},
			value(ValueKind::String),
			SnapshotSeverity::Major,
		),
		(
			value(ValueKind::String),
			OptionValue {
				repeatable: true,
				..value(ValueKind::String)
			},
			SnapshotSeverity::Minor,
		),
		(
			OptionValue {
				variadic: true,
				..value(ValueKind::String)
			},
			value(ValueKind::String),
			SnapshotSeverity::Major,
		),
		(
			value(ValueKind::String),
			OptionValue {
				variadic: true,
				..value(ValueKind::String)
			},
			SnapshotSeverity::Minor,
		),
		(
			OptionValue {
				enum_values: vec!["a".to_string(), "b".to_string()],
				..value(ValueKind::Enum)
			},
			OptionValue {
				enum_values: vec!["a".to_string()],
				..value(ValueKind::Enum)
			},
			SnapshotSeverity::Major,
		),
		(
			OptionValue {
				enum_values: vec!["a".to_string()],
				..value(ValueKind::Enum)
			},
			OptionValue {
				enum_values: vec!["a".to_string(), "b".to_string()],
				..value(ValueKind::Enum)
			},
			SnapshotSeverity::Minor,
		),
		(
			value(ValueKind::Enum),
			value(ValueKind::String),
			SnapshotSeverity::Minor,
		),
		(
			value(ValueKind::String),
			value(ValueKind::Enum),
			SnapshotSeverity::Major,
		),
		(
			value(ValueKind::Flag),
			value(ValueKind::String),
			SnapshotSeverity::Major,
		),
		(
			value(ValueKind::String),
			value(ValueKind::String),
			SnapshotSeverity::None,
		),
	];

	for (before_value, after_value, expected) in cases {
		let mut before_command = command_node("run");
		let mut after_command = command_node("run");
		before_command
			.options
			.push(option("--format", before_value));
		after_command.options.push(option("--format", after_value));
		let report = diff_command_snapshots(
			&command_snapshot(vec![before_command]),
			&command_snapshot(vec![after_command]),
		);
		assert_eq!(report.recommendation, expected);
	}
}

#[test]
fn diff_classifies_positional_contract_changes() {
	let mut before_command = command_node("run");
	let mut after_command = command_node("run");
	before_command
		.positionals
		.push(positional("input", value(ValueKind::String)));
	let report = diff_command_snapshots(
		&command_snapshot(vec![before_command.clone()]),
		&command_snapshot(vec![after_command.clone()]),
	);
	assert_eq!(
		report.changes[0].kind,
		SnapshotChangeKind::PositionalRemoved
	);

	after_command
		.positionals
		.push(positional("input", value(ValueKind::String)));
	after_command.positionals.push(positional(
		"extra",
		OptionValue {
			required: true,
			..value(ValueKind::String)
		},
	));
	let report = diff_command_snapshots(
		&command_snapshot(vec![before_command.clone()]),
		&command_snapshot(vec![after_command.clone()]),
	);
	assert_eq!(report.recommendation, SnapshotSeverity::Major);
	assert!(
		report
			.changes
			.iter()
			.any(|change| change.kind == SnapshotChangeKind::PositionalAdded)
	);

	let mut renamed_command = command_node("run");
	renamed_command
		.positionals
		.push(positional("source", value(ValueKind::String)));
	let report = diff_command_snapshots(
		&command_snapshot(vec![before_command]),
		&command_snapshot(vec![renamed_command]),
	);
	assert_eq!(
		report.changes[0].kind,
		SnapshotChangeKind::PositionalChanged
	);
}
