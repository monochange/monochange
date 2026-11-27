use clap::Arg;
use clap::ArgAction;
use clap::Command;

use super::*;

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
		max_bump: SnapshotSeverity::Major,
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
