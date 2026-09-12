use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::CliSnapshotCommandDefinition;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageCliDefinition;
use monochange_snapshot::CommandSnapshot;
use monochange_snapshot::SNAPSHOT_SCHEMA_VERSION;

use crate::release_artifacts::root_relative;

/// Directory (relative to the workspace root) holding the committed
/// command-surface baselines. The rest of `.monochange/` is committed release
/// state, so baselines ride along with the release commit.
pub(crate) const CLI_SNAPSHOT_BASELINE_DIR: &str = ".monochange/cli-snapshots";

/// How a CLI snapshot comparison ended for one package.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) enum CliSnapshotBaseline {
	/// No baseline has been committed for this CLI yet.
	Missing,
	/// The baseline file exists but is not a valid snapshot document.
	Invalid(String),
	/// The baseline was captured with a different snapshot schema version.
	Stale {
		baseline: CommandSnapshot,
		expected_schema_version: String,
	},
	/// The baseline is readable and schema-compatible.
	Current(CommandSnapshot),
}

/// A captured CLI snapshot with the command that produced it.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct CapturedCliSnapshot {
	pub snapshot: CommandSnapshot,
}

/// Return the workspace-relative baseline path for a registered CLI.
#[must_use]
pub(crate) fn cli_snapshot_baseline_path(name: &str) -> PathBuf {
	Path::new(CLI_SNAPSHOT_BASELINE_DIR).join(format!("{name}.json"))
}

/// Load and classify the committed baseline for a registered CLI.
pub(crate) fn read_cli_snapshot_baseline(root: &Path, name: &str) -> CliSnapshotBaseline {
	let path = root.join(cli_snapshot_baseline_path(name));
	let Ok(contents) = fs::read_to_string(&path) else {
		return CliSnapshotBaseline::Missing;
	};
	let snapshot: CommandSnapshot = match serde_json::from_str(&contents) {
		Ok(snapshot) => snapshot,
		Err(error) => return CliSnapshotBaseline::Invalid(error.to_string()),
	};
	if snapshot.schema_version == SNAPSHOT_SCHEMA_VERSION {
		CliSnapshotBaseline::Current(snapshot)
	} else {
		CliSnapshotBaseline::Stale {
			expected_schema_version: SNAPSHOT_SCHEMA_VERSION.to_string(),
			baseline: snapshot,
		}
	}
}

/// Write a captured snapshot as the committed baseline for `name`.
pub(crate) fn save_cli_snapshot_baseline(
	root: &Path,
	name: &str,
	snapshot: &CommandSnapshot,
) -> MonochangeResult<PathBuf> {
	let path = root.join(cli_snapshot_baseline_path(name));
	if let Some(parent) = path.parent()
		&& let Err(error) = fs::create_dir_all(parent)
	{
		return Err(MonochangeError::Io(format!(
			"failed to create {}: {error}",
			parent.display()
		)));
	}
	let contents = serde_json::to_string_pretty(snapshot)
		// patch-coverage:ignore-start -- CommandSnapshot contains only strings, structs, and Vecs, so serialization cannot fail.
		.map_err(|error| MonochangeError::Io(format!("failed to encode cli snapshot: {error}")))?;
	// patch-coverage:ignore-end
	fs::write(&path, contents).map_err(|error| {
		MonochangeError::Io(format!("failed to write {}: {error}", path.display()))
	})?;
	Ok(path)
}

/// Run the configured snapshot command for `cli` and parse its stdout as a
/// normalized command-surface snapshot.
pub(crate) fn capture_cli_snapshot(
	root: &Path,
	package_id: &str,
	cli: &PackageCliDefinition,
) -> MonochangeResult<CapturedCliSnapshot> {
	let definition = &cli.snapshot;
	let cwd = resolve_snapshot_cwd(root, definition);
	let output = if let Some(shell_binary) = definition.shell.shell_binary() {
		ProcessCommand::new(shell_binary)
			.arg("-c")
			.arg(&definition.command)
			.current_dir(&cwd)
			.output()
	} else {
		let parts = shlex::split(&definition.command).ok_or_else(|| {
			MonochangeError::Config(format!(
				"failed to parse cli snapshot command `{}`",
				definition.command
			))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(format!(
				"cli snapshot command for package `{package_id}` must not be empty"
			)));
		};
		ProcessCommand::new(program)
			.args(args)
			.current_dir(&cwd)
			.output()
	};
	let output = output.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to run cli snapshot command `{}` for package `{package_id}`: {error}",
			definition.command
		))
	})?;

	if !output.status.success() {
		return Err(MonochangeError::Config(format!(
			"cli snapshot command `{}` for package `{package_id}` failed: {}{}",
			definition.command,
			output.status,
			stderr_excerpt(&output.stderr)
		)));
	}

	let stdout = String::from_utf8_lossy(&output.stdout);
	let snapshot: CommandSnapshot = serde_json::from_str(stdout.trim()).map_err(|error| {
		MonochangeError::Config(format!(
			"cli snapshot command `{}` for package `{package_id}` did not print a valid command-surface snapshot: {error}",
			definition.command
		))
	})?;
	if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
		return Err(MonochangeError::Config(format!(
			"cli snapshot for `{}` was captured with schema version `{}` but this monochange expects `{SNAPSHOT_SCHEMA_VERSION}`; regenerate the snapshot with a compatible tool",
			cli.name, snapshot.schema_version
		)));
	}

	Ok(CapturedCliSnapshot { snapshot })
}

fn resolve_snapshot_cwd(root: &Path, definition: &CliSnapshotCommandDefinition) -> PathBuf {
	match &definition.cwd {
		Some(cwd) => root.join(cwd),
		None => root.to_path_buf(),
	}
}

fn stderr_excerpt(stderr: &[u8]) -> String {
	let lossy = String::from_utf8_lossy(stderr);
	let stderr = lossy.trim();
	if stderr.is_empty() {
		String::new()
	} else {
		format!("\nstderr:\n{stderr}")
	}
}

/// Render a human-readable reason for why a baseline cannot be diffed.
pub(crate) fn describe_cli_snapshot_baseline(baseline: &CliSnapshotBaseline) -> String {
	match baseline {
		CliSnapshotBaseline::Missing => "no committed baseline; run `monochange snapshot --save` for this package during release".to_string(),
		CliSnapshotBaseline::Invalid(error) => format!("committed baseline is not a valid snapshot document: {error}"),
		CliSnapshotBaseline::Stale {
			baseline,
			expected_schema_version,
		} => format!(
			"committed baseline uses snapshot schema version `{}` but this monochange expects `{expected_schema_version}`",
			baseline.schema_version
		),
		CliSnapshotBaseline::Current(_) => "baseline is current".to_string(),
	}
}

pub(crate) fn cli_snapshot_root_relative(root: &Path, name: &str) -> PathBuf {
	root_relative(root, &cli_snapshot_baseline_path(name))
}

/// `monochange snapshot --package <id> [--save]`: capture the configured
/// package CLI snapshot, optionally persisting it as the committed baseline.
pub(crate) fn run_package_snapshot(
	root: &Path,
	package_id: &str,
	save: bool,
	view: monochange_snapshot::SnapshotView,
) -> MonochangeResult<String> {
	let configuration = monochange_config::load_workspace_configuration(root)?;
	let definition = configuration
		.package_by_id(package_id)
		.ok_or_else(|| MonochangeError::Config(format!("unknown package `{package_id}`")))?;
	let cli = definition.cli.as_ref().ok_or_else(|| {
		MonochangeError::Config(format!(
			"package `{package_id}` does not register a cli; add `[package.{package_id}].cli` to monochange.toml"
		))
	})?;
	let captured = capture_cli_snapshot(root, package_id, cli)?;
	if save {
		let path = save_cli_snapshot_baseline(root, &cli.name, &captured.snapshot)?;
		return Ok(format!(
			"Saved cli snapshot for `{}` to {}",
			cli.name,
			root_relative(root, &path).display()
		));
	}
	captured
		.snapshot
		.view(view)
		.to_json()
		.map_err(|error| MonochangeError::Config(format!("failed to render snapshot: {error}")))
}

/// `monochange snapshot --list`: print every registered package CLI with its
/// baseline status.
pub(crate) fn list_registered_clis(root: &Path) -> MonochangeResult<String> {
	let configuration = monochange_config::load_workspace_configuration(root)?;
	let mut lines = Vec::new();
	for definition in &configuration.packages {
		let Some(cli) = definition.cli.as_ref() else {
			continue;
		};
		let baseline = read_cli_snapshot_baseline(root, &cli.name);
		let status = match &baseline {
			CliSnapshotBaseline::Current(_) => "baseline current",
			CliSnapshotBaseline::Missing => "baseline missing",
			CliSnapshotBaseline::Invalid(_) | CliSnapshotBaseline::Stale { .. } => {
				"baseline stale or invalid"
			}
		};
		lines.push(format!(
			"{}\t{}\t{}\t{}",
			cli.name, definition.id, cli.snapshot.command, status
		));
	}
	if lines.is_empty() {
		return Ok(
			"No packages register a cli. Add `[package.<id>].cli = { name = \"<binary>\", snapshot = \"<command>\" }` to monochange.toml."
				.to_string(),
		);
	}
	lines.insert(0, "name\tpackage\tsnapshot command\tbaseline".to_string());
	Ok(lines.join("\n"))
}

#[cfg(test)]
#[path = "__tests__/cli_surface_tests.rs"]
mod tests;
