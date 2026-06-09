#![allow(clippy::large_futures)]
#![allow(clippy::disallowed_methods)]
//! Integration tests for `monochange help` subcommand output.

use std::path::PathBuf;

use insta_cmd::assert_cmd_snapshot;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::snapshot_settings;

fn workspace_root() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR"))
		.parent()
		.and_then(std::path::Path::parent)
		.unwrap_or_else(|| panic!("monochange crate should live under crates/monochange"))
		.to_path_buf()
}

fn mc_command() -> std::process::Command {
	let mut command = std::process::Command::new(insta_cmd::get_cargo_bin("monochange"));
	command.current_dir(workspace_root());
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command
}

#[test]
fn help_overview_lists_all_commands() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help"));
}

#[test]
fn run_change_help_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("run").arg("change").arg("--help"));
}

#[test]
fn run_release_help_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("run").arg("release").arg("--help"));
}

#[test]
fn help_unknown_command_shows_error_and_list() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("nonexistent"));
}

#[test]
fn help_init_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("init"));
}

#[test]
fn help_analyze_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("analyze"));
}
