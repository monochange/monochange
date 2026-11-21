#![allow(clippy::disallowed_methods)]
use std::process::Command;

use monochange_core::PrereleaseConfiguration;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::SourceProvider;
use tempfile::tempdir;

use super::*;

#[tokio::test(flavor = "multi_thread")]
async fn commit_wrapper_enforces_policy_when_enabled() {
	let repo = init_git_repo();
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["checkout", "-b", "feature/demo"]);
	write_and_commit(repo.path(), "feature.txt", "feature", "feature commit");
	let mut source = source_configuration();
	source.releases.branches = vec!["release/*".to_string()];
	source.releases.enforce_for_commit = true;
	let error = verify_release_ref_for_commit(repo.path(), Some(&source), "HEAD")
		.await
		.err()
		.unwrap_or_else(|| panic!("expected commit verification error"));

	assert!(
		error
			.to_string()
			.contains("is not reachable from any configured release branch pattern [release/*]")
	);
}

fn source_configuration() -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn init_git_repo() -> tempfile::TempDir {
	let repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	run_git(repo.path(), &["init", "-b", "main"]);
	run_git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	run_git(repo.path(), &["config", "user.name", "monochange tests"]);
	run_git(repo.path(), &["config", "commit.gpgsign", "false"]);
	write_and_commit(repo.path(), "README.md", "root", "initial commit");
	run_git(repo.path(), &["checkout", "-b", "release/production"]);
	repo
}

fn write_and_commit(root: &Path, path: &str, contents: &str, message: &str) {
	std::fs::write(root.join(path), contents)
		.unwrap_or_else(|error| panic!("write {path}: {error}"));
	run_git(root, &["add", path]);
	run_git(root, &["commit", "-m", message]);
}

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
}
