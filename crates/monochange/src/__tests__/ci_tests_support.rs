//! Shared helpers for the `ci` unit tests.

use std::path::Path;
use std::process::Command as ProcessCommand;

use monochange_core::MonochangeResult;

use crate::run_with_args_in_dir;

/// Run `monochange` with `args` inside `root`.
pub(super) fn run_mc(root: &Path, args: &[&str]) -> MonochangeResult<String> {
	let runtime = tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap();
	let mut full_args = vec![std::ffi::OsString::from("monochange")];

	full_args.extend(args.iter().map(std::ffi::OsString::from));

	runtime.block_on(run_with_args_in_dir("monochange", full_args, root))
}

/// Initialise a git repository with an `origin` remote.
pub(super) fn init_git_repo_with_origin(root: &Path, origin_url: &str) {
	ProcessCommand::new("git")
		.current_dir(root)
		.args(["init"])
		.output()
		.unwrap();
	ProcessCommand::new("git")
		.current_dir(root)
		.args(["remote", "add", "origin", origin_url])
		.output()
		.unwrap();
}
