//! Hosted release endpoint validation, responses, and error mapping.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use axum::http::StatusCode;
use httpmock::Method;
use httpmock::MockServer;
use monochange_core::HostedCommitFile;
use monochange_core::HostedCommitRequest;
use monochange_core::HostedCommitResponse;
use rstest::rstest;

use super::commit_release_files;
use super::release_commit_error_status;
use super::validate_commit_paths;
use crate::github_app::GitHubAppError;

fn request_with_path(path: &str) -> HostedCommitRequest {
	HostedCommitRequest {
		provider: "github".to_string(),
		owner: "acme".to_string(),
		repository: "actions".to_string(),
		branch: "monochange/release/release".to_string(),
		base_branch: Some("main".to_string()),
		base_commit: "base-sha".to_string(),
		subject: "chore(release): prepare release".to_string(),
		body: String::new(),
		files: vec![HostedCommitFile {
			path: path.to_string(),
			content: Some("{}".to_string()),
		}],
		dry_run: false,
		idempotency_key: None,
	}
}

#[rstest]
#[case::release_record(".monochange/releases/829c44b7a1ee8674/release.json")]
#[case::changeset_deletion(".changeset/feature.md")]
#[case::package_manifest("crates/core/Cargo.toml")]
fn release_managed_paths_are_accepted(#[case] path: &str) {
	assert_eq!(validate_commit_paths(&request_with_path(path)), Ok(()));
}

#[rstest]
#[case::absolute("/home/runner/work/actions/actions/.monochange/releases/abc/release.json")]
#[case::parent_traversal("../outside.md")]
#[case::git_directory(".git/config")]
#[case::workflow(".github/workflows/release.yml")]
fn escaping_paths_are_rejected(#[case] path: &str) {
	assert!(validate_commit_paths(&request_with_path(path)).is_err());
}

#[test]
fn moved_branches_are_conflicts() {
	let moved = |branch: &str| (branch.to_string(), "new".to_string(), "old".to_string());
	let (branch, actual, expected) = moved("monochange/release/release");
	assert_eq!(
		release_commit_error_status(&GitHubAppError::BranchMoved(branch, actual, expected)),
		StatusCode::CONFLICT
	);
	let (branch, actual, expected) = moved("main");
	assert_eq!(
		release_commit_error_status(&GitHubAppError::BaseMoved(branch, actual, expected)),
		StatusCode::CONFLICT
	);
	assert_eq!(
		release_commit_error_status(&GitHubAppError::NotConfigured),
		StatusCode::SERVICE_UNAVAILABLE
	);
	assert_eq!(
		release_commit_error_status(&GitHubAppError::UnverifiedCommit),
		StatusCode::BAD_GATEWAY
	);
}

/// Serve a GitHub API that creates the release branch with `verification`.
fn github_creating_release_branch(verification: &serde_json::Value) -> MockServer {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/ref/heads/monochange/release/release");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/blobs");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "blob-sha" }));
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/trees");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "tree-sha" }));
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/commits");
		then.status(201).json_body(serde_json::json!({
			"sha": "release-sha",
			"verification": verification,
		}));
	});
	server.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/refs");
		then.status(201);
	});
	server
}

#[tokio::test]
async fn release_commit_response_reports_github_verification() {
	let server = github_creating_release_branch(
		&serde_json::json!({ "verified": false, "reason": "unsigned" }),
	);

	let response = commit_release_files(
		&reqwest::Client::new(),
		&server.base_url(),
		"installation-token",
		&request_with_path("crates/core/Cargo.toml"),
	)
	.await
	.unwrap();

	assert_eq!(
		response,
		HostedCommitResponse {
			commit: Some("release-sha".to_string()),
			verified: false,
			status: Some("completed".to_string()),
			message: Some("unsigned".to_string()),
		}
	);
}

#[tokio::test]
async fn stale_release_commit_is_a_conflict() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/ref/heads/monochange/release/release");
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": "previous-release-sha" } }));
	});
	server.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/ref/heads/main");
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": "newer-base-sha" } }));
	});

	let (status, body) = commit_release_files(
		&reqwest::Client::new(),
		&server.base_url(),
		"installation-token",
		&request_with_path("crates/core/Cargo.toml"),
	)
	.await
	.unwrap_err();

	assert_eq!(status, StatusCode::CONFLICT);
	assert!(
		body.message
			.starts_with("base branch `main` moved to `newer-base-sha`"),
		"{}",
		body.message
	);
}
