use std::fs;

use monochange_test_helpers::git::git;
use temp_env::with_vars;
use tempfile::tempdir;

use super::*;

fn init_repo() -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(tempdir.path().join("README.md"), "hello\n")
		.unwrap_or_else(|error| panic!("write fixture: {error}"));

	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "initial"]);
	git(tempdir.path(), &["branch", "-M", "main"]);

	tempdir
}

#[test]
fn detect_uses_github_pr_environment_when_refs_exist() {
	let tempdir = init_repo();
	git(tempdir.path(), &["branch", "feature-branch"]);

	let frame = with_vars(
		[
			("GITHUB_EVENT_NAME", Some("pull_request")),
			("GITHUB_HEAD_REF", Some("feature-branch")),
			("GITHUB_BASE_REF", Some("main")),
		],
		|| ChangeFrame::detect(tempdir.path()),
	)
	.unwrap_or_else(|error| panic!("detect frame: {error}"));

	assert_eq!(
		frame,
		ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature-branch".to_string(),
		}
	);
}
