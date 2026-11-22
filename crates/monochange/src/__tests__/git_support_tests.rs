#![allow(clippy::disallowed_methods)]
use std::fs;
use std::process::Stdio;

use tempfile::tempdir;

use super::*;

fn git(root: &Path, args: &[&str]) {
	let output = monochange_core::git::git_command(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn git_output(root: &Path, args: &[&str]) -> String {
	let output = monochange_core::git::git_command(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git stdout utf8: {error}"))
		.trim()
		.to_string()
}

fn init_git_repo(root: &Path) {
	git(root, &["init", "-b", "main"]);
	git(root, &["config", "user.name", "monochange tests"]);
	git(root, &["config", "user.email", "monochange@example.com"]);
	git(root, &["config", "commit.gpgsign", "false"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_stage_paths_returns_ok_when_all_paths_are_non_stageable() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(root.join(".gitignore"), ".monochange/\n")
		.unwrap_or_else(|error| panic!("write .gitignore: {error}"));
	fs::write(root.join("tracked.txt"), "tracked\n")
		.unwrap_or_else(|error| panic!("write tracked file: {error}"));
	fs::create_dir_all(root.join(".monochange/releases"))
		.unwrap_or_else(|error| panic!("create release state: {error}"));
	fs::write(root.join(".monochange/releases/release.json"), "{}\n")
		.unwrap_or_else(|error| panic!("write release state: {error}"));
	init_git_repo(root);
	git(root, &["add", "."]);

	git(root, &["add", "-f", ".monochange/releases/release.json"]);
	git(root, &["commit", "-m", "initial"]);
	fs::create_dir_all(root.join(".monochange/local"))
		.unwrap_or_else(|error| panic!("create .monochange: {error}"));
	fs::write(root.join(".monochange/local/release-manifest.json"), "{}\n")
		.unwrap_or_else(|error| panic!("write release manifest: {error}"));
	fs::write(
		root.join(".monochange/releases/release.json"),
		"{\"released\":true}\n",
	)
	.unwrap_or_else(|error| panic!("update release state: {error}"));
	fs::write(root.join("untracked.txt"), "untracked\n")
		.unwrap_or_else(|error| panic!("write untracked file: {error}"));

	git_stage_paths(
		root,
		&[
			PathBuf::from(".monochange/local/release-manifest.json"),
			PathBuf::from(".monochange/releases/release.json"),
			PathBuf::from("untracked.txt"),
			PathBuf::from(".changeset/missing.md"),
		],
	)
	.await
	.unwrap_or_else(|error| panic!("git stage paths: {error}"));

	assert_eq!(
		git_output(root, &["diff", "--cached", "--name-only"]),
		".monochange/releases/release.json\nuntracked.txt"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn git_changed_paths_lists_what_stage_all_would_commit() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join(".gitignore"), "/target\n")
		.unwrap_or_else(|error| panic!("write gitignore: {error}"));
	fs::write(root.join("kept.txt"), "kept\n")
		.unwrap_or_else(|error| panic!("write kept: {error}"));
	fs::write(root.join("edited.txt"), "before\n")
		.unwrap_or_else(|error| panic!("write edited: {error}"));
	fs::write(root.join("removed.txt"), "removed\n")
		.unwrap_or_else(|error| panic!("write removed: {error}"));
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "initial"]);

	fs::write(root.join("edited.txt"), "after\n")
		.unwrap_or_else(|error| panic!("edit file: {error}"));
	fs::remove_file(root.join("removed.txt"))
		.unwrap_or_else(|error| panic!("remove file: {error}"));
	fs::write(root.join("staged.txt"), "staged\n")
		.unwrap_or_else(|error| panic!("write staged: {error}"));
	git(root, &["add", "staged.txt"]);
	fs::write(root.join("untracked.txt"), "untracked\n")
		.unwrap_or_else(|error| panic!("write untracked: {error}"));
	fs::create_dir_all(root.join("target"))
		.unwrap_or_else(|error| panic!("create target: {error}"));
	fs::write(root.join("target/ignored.txt"), "ignored\n")
		.unwrap_or_else(|error| panic!("write ignored: {error}"));

	let paths = git_changed_paths(root)
		.await
		.unwrap_or_else(|error| panic!("list changed paths: {error}"));

	assert_eq!(
		paths,
		["edited.txt", "removed.txt", "staged.txt", "untracked.txt"]
			.map(PathBuf::from)
			.to_vec()
	);
}
