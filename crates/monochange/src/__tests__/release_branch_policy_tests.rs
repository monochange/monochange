#![allow(clippy::disallowed_methods)]
use std::process::Command;

use monochange_core::CutBranchRetention;
use monochange_core::PrereleaseConfiguration;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ReleasePullRequestStrategy;
use monochange_core::SourceProvider;
use serde_json::json;
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

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_accepts_published_cut_record_and_tag() {
	let repo = init_repo();
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&cut_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let report = verify_deleted_cut_retry(repo.path(), &cut_source_configuration(), "v1.1.0")
		.await
		.unwrap_or_else(|| panic!("expected deleted cut retry report"));

	assert_eq!(report.matched_branch, "release/v1.1.0 (deleted cut branch)");
	assert_eq!(
		report.commit,
		git_output(repo.path(), &["rev-parse", "v1.1.0"])
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_commit_without_release_metadata() {
	let repo = init_repo();
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_and_commit(
		repo.path(),
		"release.txt",
		"release",
		"chore: branch commit without release metadata",
	);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_record_without_release_tag() {
	let repo = init_repo();
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&cut_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_release_tag_pointing_elsewhere() {
	let repo = init_repo();
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&cut_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_record_from_an_ancestor_commit() {
	let repo = init_repo();
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&cut_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	write_and_commit(
		repo.path(),
		"release.txt",
		"release",
		"chore: release commit after the record",
	);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_synced_bookkeeping_record() {
	let repo = init_repo();
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	let mut record = release_record(Some(&cut_base), "v1.1.0", "1.1.0");
	record["synced_from_commit"] = json!(cut_base);
	write_release_record(repo.path(), &record, "v1.1.0");
	run_git(repo.path(), &["tag", "v1.1.0"]);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_recorded_base_that_is_not_an_ancestor() {
	let repo = init_repo();
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	run_git(repo.path(), &["checkout", "main"]);
	write_and_commit(
		repo.path(),
		"main-only.txt",
		"main",
		"feat: unrelated main commit",
	);
	let unrelated_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&unrelated_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_recorded_base_outside_the_configured_base() {
	let repo = init_repo();
	run_git(repo.path(), &["checkout", "-b", "side"]);
	write_and_commit(repo.path(), "side.txt", "side", "feat: side branch base");
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&cut_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_record_without_frozen_cut_base() {
	let repo = init_repo();
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(None, "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_cut_branch_outside_the_release_patterns() {
	let repo = init_repo();
	let cut_base = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some(&cut_base), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);
	let mut source = cut_source_configuration();
	source.releases.branches = vec!["main".to_string()];

	let error = deleted_cut_retry_error(repo.path(), &source, &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [main]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn deleted_cut_branch_retry_rejects_unresolvable_recorded_cut_base() {
	let repo = init_repo();
	run_git(repo.path(), &["checkout", "-b", "release/v1.1.0"]);
	write_release_record(
		repo.path(),
		&release_record(Some("does-not-exist"), "v1.1.0", "1.1.0"),
		"v1.1.0",
	);
	run_git(repo.path(), &["tag", "v1.1.0"]);
	let release_commit = git_output(repo.path(), &["rev-parse", "HEAD"]);
	run_git(repo.path(), &["checkout", "main"]);
	run_git(repo.path(), &["branch", "-D", "release/v1.1.0"]);

	let error =
		deleted_cut_retry_error(repo.path(), &cut_source_configuration(), &release_commit).await;

	assert!(
		error.contains("is not reachable from any configured release branch pattern [release/*]"),
		"{error}"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_release_branch_pattern_is_reported() {
	let repo = init_repo();
	let mut source = cut_source_configuration();
	source.releases.branches = vec!["release/[".to_string()];

	let error = verify_release_ref_for_publish(
		repo.path(),
		Some(&source),
		&PrereleaseConfiguration::default(),
		"HEAD",
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected invalid branch pattern error"));

	assert!(
		error
			.to_string()
			.contains("invalid [source.releases].branches pattern `release/[`"),
		"{error}"
	);
}

async fn verify_deleted_cut_retry(
	root: &Path,
	source: &SourceConfiguration,
	ref_name: &str,
) -> Option<ReleaseBranchVerificationReport> {
	verify_release_ref_for_publish(
		root,
		Some(source),
		&PrereleaseConfiguration::default(),
		ref_name,
	)
	.await
	.unwrap_or_else(|error| panic!("expected deleted cut retry for `{ref_name}`: {error}"))
}

async fn deleted_cut_retry_error(
	root: &Path,
	source: &SourceConfiguration,
	ref_name: &str,
) -> String {
	verify_release_ref_for_publish(
		root,
		Some(source),
		&PrereleaseConfiguration::default(),
		ref_name,
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected deleted cut retry for `{ref_name}` to be rejected"))
	.to_string()
}

fn cut_source_configuration() -> SourceConfiguration {
	let mut source = source_configuration();
	source.releases.branches = vec!["release/*".to_string()];
	source.releases.enforce_for_publish = true;
	source.pull_requests.strategy = ReleasePullRequestStrategy::Cut;
	source.pull_requests.base = "main".to_string();
	source.pull_requests.cut.branch_template = "release/{{ cut_id }}".to_string();
	source.pull_requests.cut.retention = CutBranchRetention::Delete;
	source
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

fn release_record(
	cut_base_commit: Option<&str>,
	tag_name: &str,
	version: &str,
) -> serde_json::Value {
	let mut record = json!({
		"schema_version": monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		"kind": monochange_core::RELEASE_RECORD_KIND,
		"created_at": "2026-01-01T00:00:00Z",
		"command": "monochange release",
		"version": version,
		"release_targets": [{
			"id": "core",
			"kind": "group",
			"version": version,
			"version_format": "primary",
			"tag": true,
			"release": true,
			"tag_name": tag_name,
			"members": [],
		}],
		"released_packages": ["core"],
		"changed_files": [],
	});

	if let Some(cut_base_commit) = cut_base_commit {
		record["cut_base_commit"] = json!(cut_base_commit);
	}

	record
}

fn write_release_record(root: &Path, record: &serde_json::Value, tag_name: &str) {
	let contents = serde_json::to_string_pretty(record)
		.unwrap_or_else(|error| panic!("serialize release record: {error}"));
	let path = format!(".monochange/releases/{tag_name}/release.json");
	write_and_commit(root, &path, &contents, "chore(release): record");
}

fn init_git_repo() -> tempfile::TempDir {
	let repo = init_repo();
	run_git(repo.path(), &["checkout", "-b", "release/production"]);
	repo
}

fn init_repo() -> tempfile::TempDir {
	let repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	run_git(repo.path(), &["init", "-b", "main"]);
	run_git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	run_git(repo.path(), &["config", "user.name", "monochange tests"]);
	run_git(repo.path(), &["config", "commit.gpgsign", "false"]);
	write_and_commit(repo.path(), "README.md", "root", "initial commit");
	repo
}

fn write_and_commit(root: &Path, path: &str, contents: &str, message: &str) {
	let absolute = root.join(path);

	if let Some(parent) = absolute.parent() {
		std::fs::create_dir_all(parent)
			.unwrap_or_else(|error| panic!("create {}: {error}", parent.display()));
	}

	std::fs::write(&absolute, contents).unwrap_or_else(|error| panic!("write {path}: {error}"));
	run_git(root, &["add", path]);
	run_git(root, &["commit", "-m", message]);
}

fn git_output(root: &Path, args: &[&str]) -> String {
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
	String::from_utf8_lossy(&output.stdout).trim().to_string()
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
