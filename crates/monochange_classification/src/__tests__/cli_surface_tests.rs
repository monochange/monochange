use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::CliSnapshotCommandDefinition;
use monochange_core::PackageCliDefinition;
use monochange_core::ShellConfig;
use monochange_snapshot::CommandSnapshot;
use monochange_snapshot::SNAPSHOT_SCHEMA_VERSION;
use tempfile::tempdir;

use super::*;

fn sample_snapshot(schema_version: &str) -> CommandSnapshot {
	CommandSnapshot {
		schema_version: schema_version.to_string(),
		kind: monochange_snapshot::SnapshotKind::CliSurface,
		tool: monochange_snapshot::SnapshotTool {
			name: "demo".to_string(),
			version: Some("1.0.0".to_string()),
		},
		provenance: monochange_snapshot::SnapshotProvenance {
			extractor: "clap".to_string(),
			confidence: monochange_snapshot::SnapshotConfidence::High,
		},
		standard_entrypoints: monochange_snapshot::StandardEntrypoints {
			help: monochange_snapshot::StandardEntrypoint::default(),
			version: monochange_snapshot::StandardEntrypoint::default(),
			snapshot: monochange_snapshot::StandardEntrypoint::default(),
		},
		global_options: Vec::new(),
		commands: Vec::new(),
		output_contracts: Vec::new(),
	}
}

fn write_snapshot_file(root: &Path, relative: &str, schema_version: &str) -> PathBuf {
	let path = root.join(relative);

	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}

	let snapshot = sample_snapshot(schema_version);
	fs::write(
		&path,
		serde_json::to_string_pretty(&snapshot)
			.unwrap_or_else(|error| panic!("encode snapshot: {error}")),
	)
	.unwrap_or_else(|error| panic!("write snapshot: {error}"));
	path
}

fn cli_with_command(command: &str, shell: ShellConfig, cwd: Option<&str>) -> PackageCliDefinition {
	PackageCliDefinition {
		name: "demo".to_string(),
		snapshot: CliSnapshotCommandDefinition {
			command: command.to_string(),
			cwd: cwd.map(PathBuf::from),
			shell,
		},
	}
}

#[test]
fn capture_rejects_stdout_that_is_not_a_snapshot_document() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("nope.txt"), "plain help text")
		.unwrap_or_else(|error| panic!("write file: {error}"));
	let cli = cli_with_command("cat nope.txt", ShellConfig::None, None);

	let error = capture_cli_snapshot(tempdir.path(), "demo", &cli)
		.err()
		.unwrap_or_else(|| panic!("expected capture failure"));

	assert!(
		error
			.to_string()
			.contains("did not print a valid command-surface snapshot"),
		"{error}"
	);
}

#[test]
fn capture_rejects_unparseable_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let cli = cli_with_command("cat 'unterminated", ShellConfig::None, None);

	let error = capture_cli_snapshot(tempdir.path(), "demo", &cli)
		.err()
		.unwrap_or_else(|| panic!("expected capture failure"));

	assert!(
		error
			.to_string()
			.contains("failed to parse cli snapshot command"),
		"{error}"
	);
}

#[test]
fn capture_reports_missing_programs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let cli = cli_with_command("definitely-not-a-real-binary-4b1f", ShellConfig::None, None);

	let error = capture_cli_snapshot(tempdir.path(), "demo", &cli)
		.err()
		.unwrap_or_else(|| panic!("expected capture failure"));

	assert!(
		error
			.to_string()
			.contains("failed to run cli snapshot command"),
		"{error}"
	);
}

#[test]
fn describe_baseline_explains_every_state() {
	assert_eq!(
		describe_cli_snapshot_baseline(&CliSnapshotBaseline::Missing),
		"no committed baseline; run `monochange snapshot --save` for this package during release"
	);
	assert!(
		describe_cli_snapshot_baseline(&CliSnapshotBaseline::Invalid("boom".to_string()))
			.contains("not a valid snapshot document: boom")
	);
	let stale = CliSnapshotBaseline::Stale {
		baseline: sample_snapshot("0.0"),
		expected_schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
	};
	assert!(describe_cli_snapshot_baseline(&stale).contains("uses snapshot schema version `0.0`"));
	assert_eq!(
		describe_cli_snapshot_baseline(&CliSnapshotBaseline::Current(sample_snapshot(
			SNAPSHOT_SCHEMA_VERSION
		))),
		"baseline is current"
	);
}

fn cli_registration_workspace(root: &Path, cli_line: &str) {
	let package_dir = root.join("crates/demo");
	fs::create_dir_all(&package_dir).unwrap_or_else(|error| panic!("create dir: {error}"));
	fs::write(
		package_dir.join("Cargo.toml"),
		"[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));
	let config = format!("[package.demo]\npath = \"crates/demo\"\ntype = \"cargo\"\n{cli_line}\n");
	fs::write(root.join("monochange.toml"), config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
}

#[test]
fn list_reports_guidance_when_no_cli_is_registered() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(tempdir.path(), "");

	let listing =
		list_registered_clis(tempdir.path()).unwrap_or_else(|error| panic!("list: {error}"));

	assert!(listing.contains("No packages register a cli"), "{listing}");
}

#[test]
fn list_reports_registered_clis_with_baseline_status() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(
		tempdir.path(),
		"cli = { name = \"demo\", snapshot = \"cat demo.json\" }",
	);
	write_snapshot_file(tempdir.path(), ".monochange/cli-snapshots/demo.json", "0.0");

	let listing =
		list_registered_clis(tempdir.path()).unwrap_or_else(|error| panic!("list: {error}"));

	assert!(
		listing.contains("demo\tdemo\tcat demo.json\tbaseline stale or invalid"),
		"{listing}"
	);
}

#[test]
fn package_snapshot_prints_the_captured_document() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(
		tempdir.path(),
		"cli = { name = \"demo\", snapshot = \"cat demo.json\" }",
	);
	write_snapshot_file(tempdir.path(), "demo.json", SNAPSHOT_SCHEMA_VERSION);

	let output = run_package_snapshot(
		tempdir.path(),
		"demo",
		false,
		monochange_snapshot::SnapshotView::Full,
	)
	.unwrap_or_else(|error| panic!("snapshot: {error}"));

	assert!(output.contains("\"schema_version\""), "{output}");
}

#[test]
fn package_snapshot_save_writes_the_committed_baseline() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	cli_registration_workspace(
		tempdir.path(),
		"cli = { name = \"demo\", snapshot = \"cat demo.json\" }",
	);
	write_snapshot_file(tempdir.path(), "demo.json", SNAPSHOT_SCHEMA_VERSION);

	let output = run_package_snapshot(
		tempdir.path(),
		"demo",
		true,
		monochange_snapshot::SnapshotView::Full,
	)
	.unwrap_or_else(|error| panic!("snapshot: {error}"));

	assert!(
		output.contains("Saved cli snapshot for `demo` to .monochange/cli-snapshots/demo.json"),
		"{output}"
	);
	let baseline = read_cli_snapshot_baseline(tempdir.path(), "demo");
	assert!(
		matches!(baseline, CliSnapshotBaseline::Current(_)),
		"{baseline:?}"
	);
}

#[test]
fn capture_reports_failing_commands_without_stderr() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let cli = cli_with_command("exit 3", ShellConfig::Default, None);

	let error = capture_cli_snapshot(tempdir.path(), "demo", &cli)
		.err()
		.unwrap_or_else(|| panic!("expected capture failure"));

	assert!(error.to_string().contains("failed"), "{error}");
	assert!(!error.to_string().contains("stderr:"), "{error}");
}
