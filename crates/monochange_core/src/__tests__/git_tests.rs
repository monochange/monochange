use std::fs;
use std::io;

use super::*;

fn tempdir(context: &str) -> tempfile::TempDir {
	tempfile::tempdir().unwrap_or_else(|error| panic!("{context}: {error}"))
}

fn git(root: &Path, args: &[&str]) {
	let status = git_command(root)
		.args(["-c", "commit.gpgsign=false"])
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed with {status}");
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
	let output = git_command(root)
		.args(["-c", "commit.gpgsign=false"])
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed with {output:?}"
	);
	String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn git_commit_paths_stdin_command_reads_message_from_stdin() {
	let command = git_commit_paths_stdin_command(Path::new("."), true);
	let args = command
		.get_args()
		.map(|arg| arg.to_string_lossy().into_owned())
		.collect::<Vec<_>>();

	assert_eq!(args, vec!["commit", "--no-verify", "--file", "-"]);
}

#[test]
fn run_git_commit_message_reports_create_temp_file_diagnostics() {
	let tempdir = tempdir("create parent");
	let missing_nested_root = tempdir.path().join("missing-parent").join("repo");
	let error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message(
			&missing_nested_root,
			&CommitMessage {
				subject: "chore(release): prepare release".to_string(),
				body: Some("release body".to_string()),
			},
			"commit release pull request changes",
			false,
		))
		.err()
		.unwrap_or_else(|| panic!("expected commit failure"))
		.to_string();

	assert!(error.contains("could not create temporary git commit message file"));
	assert!(error.contains("phase: creating temporary commit message file"));
	assert!(error.contains("temporary message file: <not yet created>"));
}

#[test]
fn run_git_commit_message_reports_write_and_flush_diagnostics() {
	let tempdir = tempdir("create repo");
	let root = tempdir.path();
	let message = CommitMessage {
		subject: "chore(release): prepare release".to_string(),
		body: Some("release body".to_string()),
	};

	let write_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message_with_io(
			root,
			&message,
			"commit release pull request changes",
			false,
			|_, _| Err(io::Error::other("write failed")),
			flush_git_commit_message_file,
		))
		.err()
		.unwrap_or_else(|| panic!("expected write failure"))
		.to_string();
	let flush_error = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(run_git_commit_message_with_io(
			root,
			&message,
			"commit release pull request changes",
			false,
			|_, _| Ok(()),
			|_| Err(io::Error::other("flush failed")),
		))
		.err()
		.unwrap_or_else(|| panic!("expected flush failure"))
		.to_string();

	assert!(write_error.contains("could not write temporary git commit message file"));
	assert!(write_error.contains("phase: writing temporary commit message file"));
	assert!(write_error.contains("temporary message file:"));
	assert!(flush_error.contains("could not flush temporary git commit message file"));
	assert!(flush_error.contains("phase: flushing temporary commit message file"));
	assert!(flush_error.contains("temporary message file:"));
}

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::disallowed_methods)]
async fn run_commit_command_allow_nothing_to_commit_executes_git_commit() {
	let repo = tempdir("create git repo");
	git(repo.path(), &["init"]);
	git(repo.path(), &["config", "user.email", "test@example.com"]);
	git(repo.path(), &["config", "user.name", "Test User"]);
	git(repo.path(), &["config", "commit.gpgsign", "false"]);
	fs::write(repo.path().join("README.md"), "hello\n")
		.unwrap_or_else(|error| panic!("write README: {error}"));
	git(repo.path(), &["add", "README.md"]);

	let mut command = git_command(repo.path());
	command.args(["commit", "-m", "initial"]);
	run_commit_command_allow_nothing_to_commit(command, "commit changes")
		.await
		.unwrap_or_else(|error| panic!("commit changes: {error}"));

	let subject = git_stdout(repo.path(), &["log", "-1", "--pretty=%s"]);
	assert_eq!(subject.trim(), "initial");
}
