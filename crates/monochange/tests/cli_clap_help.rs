#![allow(clippy::disallowed_methods)]
//! Integration snapshots for clap-rendered help output.
//!
//! These snapshots intentionally render help through clap itself. They are the
//! safety net for refactoring monochange help away from hand-maintained text and
//! toward clap-owned command definitions, doc comments, and clap attributes.

use std::path::PathBuf;

use clap::ColorChoice;
use monochange_test_helpers::snapshot_settings;

fn workspace_root() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.and_then(std::path::Path::parent)
		.unwrap_or_else(|| panic!("monochange crate should live under crates/monochange"))
		.to_path_buf()
}

struct CurrentDirGuard {
	previous: PathBuf,
}

impl CurrentDirGuard {
	fn enter(path: &PathBuf) -> Self {
		let previous = std::env::current_dir()
			.unwrap_or_else(|error| panic!("read current directory: {error}"));
		std::env::set_current_dir(path)
			.unwrap_or_else(|error| panic!("set current directory to {}: {error}", path.display()));
		Self { previous }
	}
}

impl Drop for CurrentDirGuard {
	fn drop(&mut self) {
		std::env::set_current_dir(&self.previous).unwrap_or_else(|error| {
			panic!(
				"restore current directory to {}: {error}",
				self.previous.display()
			)
		});
	}
}

fn visible_help_paths(command: &clap::Command) -> Vec<Vec<String>> {
	let mut paths = Vec::new();
	let mut current = Vec::new();
	collect_visible_help_paths(command, &mut current, &mut paths);
	paths
}

fn collect_visible_help_paths(
	command: &clap::Command,
	current: &mut Vec<String>,
	paths: &mut Vec<Vec<String>>,
) {
	paths.push(current.clone());

	for subcommand in command
		.get_subcommands()
		.filter(|command| !command.is_hide_set())
	{
		current.push(subcommand.get_name().to_string());
		collect_visible_help_paths(subcommand, current, paths);
		current.pop();
	}
}

fn command_path_label(path: &[String]) -> String {
	if path.is_empty() {
		return "monochange".to_string();
	}

	format!("monochange {}", path.join(" "))
}

fn snapshot_name_for_path(path: &[String]) -> String {
	let label = command_path_label(path);
	let sanitized = label
		.chars()
		.map(|character| {
			match character {
				'a'..='z' | '0'..='9' => character,
				'A'..='Z' => character.to_ascii_lowercase(),
				_ => '_',
			}
		})
		.collect::<String>();

	format!("clap_long_help__{sanitized}")
}

fn command_at_path<'command>(
	mut command: &'command mut clap::Command,
	path: &[String],
) -> &'command mut clap::Command {
	for segment in path {
		command = command
			.find_subcommand_mut(segment)
			.unwrap_or_else(|| panic!("expected subcommand path segment `{segment}`"));
	}

	command
}

fn render_long_help_for_path(path: &[String]) -> String {
	let mut command = monochange::build_command("monochange")
		.color(ColorChoice::Always)
		.term_width(100);
	command.build();

	let command = command_at_path(&mut command, path);
	command.render_long_help().ansi().to_string()
}

fn render_help_snapshot(path: &[String], version: &str) -> String {
	let label = command_path_label(path);
	let help = render_long_help_for_path(path);

	format!("Version: {version}\nCommand: {label}\n\n{help}")
}

#[test]
fn clap_long_help_snapshots_cover_every_visible_command_level() {
	let settings = snapshot_settings();
	let _guard = settings.bind_to_scope();
	let workspace_root = workspace_root();
	let _current_dir = CurrentDirGuard::enter(&workspace_root);

	let command = monochange::build_command("monochange");
	let version = command
		.get_version()
		.unwrap_or_else(|| panic!("monochange command should expose a clap version"));
	let paths = visible_help_paths(&command);

	for path in paths {
		insta::assert_snapshot!(
			snapshot_name_for_path(&path),
			render_help_snapshot(&path, version)
		);
	}
}
