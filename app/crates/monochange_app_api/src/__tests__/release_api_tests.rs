//! Hosted release endpoint validation, responses, and error mapping.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::OnceLock;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use httpmock::Method;
use httpmock::MockServer;
use monochange_core::HostedCommitFile;
use monochange_core::HostedCommitRequest;
use monochange_core::HostedCommitResponse;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;
use rstest::rstest;
use tower::ServiceExt;

use super::commit_release_files;
use super::release_commit_error_status;
use super::validate_branch_names;
use super::validate_commit_paths;
use crate::AppSecrets;
use crate::AppState;
use crate::api_router;
use crate::github_app::GitHubAppAuth;
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

// ── POST /api/release-commits through the real router ──

const API_TOKEN: &str = "test-only-api-token";

fn test_private_key() -> &'static str {
	static TEST_KEY: OnceLock<String> = OnceLock::new();
	TEST_KEY.get_or_init(|| {
		RsaPrivateKey::new(&mut OsRng, 2048)
			.unwrap()
			.to_pkcs1_pem(LineEnding::LF)
			.unwrap()
			.to_string()
	})
}

/// App state with `acme/actions` connected to installation 1001, the GitHub
/// App pointed at `github`, and `api_token` as the configured `MONOCHANGE_TOKEN`.
async fn release_api_state(github: &MockServer, api_token: &str) -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	for statement in [
		"INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'acme-admin', 'test-only-user-token')",
		"INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'acme', 'Organization')",
		"INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'acme/actions', 0)",
	] {
		sqlx::query(statement).execute(&db).await.unwrap();
	}
	let secrets: AppSecrets =
		serde_json::from_value(serde_json::json!({ "monochange_token": api_token })).unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_app = Some(GitHubAppAuth::new(
		"123",
		test_private_key(),
		"test-only-webhook-secret",
		&github.base_url(),
	));
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/app/installations/1001/access_tokens");
		then.status(201)
			.json_body(serde_json::json!({ "token": "installation-token" }));
	});
	state
}

fn mock_branch(github: &MockServer, branch: &str, sha: &str) {
	let path = format!("/repos/acme/actions/git/ref/heads/{branch}");
	github.mock(|when, then| {
		when.method(Method::GET).path(path);
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": sha } }));
	});
}

async fn post_release_commit(
	state: &AppState,
	bearer: &str,
	request: &HostedCommitRequest,
) -> (StatusCode, serde_json::Value) {
	let response = api_router(state.clone())
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/api/release-commits")
				.header("authorization", format!("Bearer {bearer}"))
				.header("content-type", "application/json")
				.body(Body::from(serde_json::to_vec(request).unwrap()))
				.unwrap(),
		)
		.await
		.unwrap();
	let status = response.status();
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	(status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn empty_api_token_secret_requires_oidc() {
	let secrets: AppSecrets =
		serde_json::from_value(serde_json::json!({ "monochange_token": "" })).unwrap();
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	assert_eq!(AppState::new(db, secrets).unwrap().api_token, None);
}

#[tokio::test]
async fn release_commit_endpoint_refreshes_an_existing_release_branch() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	mock_branch(
		&github,
		"monochange/release/release",
		"previous-release-sha",
	);
	mock_branch(&github, "main", "base-sha");
	github.mock(|when, then| {
		when.method(Method::GET).path("/repos/acme/actions");
		then.status(200)
			.json_body(serde_json::json!({ "default_branch": "main" }));
	});
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/blobs");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "blob-sha" }));
	});
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/trees");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "tree-sha" }));
	});
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/commits");
		then.status(201).json_body(serde_json::json!({
			"sha": "release-sha",
			"verification": { "verified": true, "reason": "valid" },
		}));
	});
	let update = github.mock(|when, then| {
		when.method(Method::PATCH)
			.path("/repos/acme/actions/git/refs/heads/monochange/release/release")
			.json_body(serde_json::json!({ "sha": "release-sha", "force": true }));
		then.status(200)
			.json_body(serde_json::json!({ "object": { "sha": "release-sha" } }));
	});

	let (status, body) = post_release_commit(
		&state,
		API_TOKEN,
		&request_with_path(".monochange/releases/829c44b7a1ee8674/release.json"),
	)
	.await;

	assert_eq!(status, StatusCode::OK, "{body}");
	assert_eq!(
		body,
		serde_json::json!({
			"commit": "release-sha",
			"verified": true,
			"status": "completed",
			"message": "valid",
		})
	);
	update.assert();
}

#[rstest]
#[case::stale_base(Some("main"), "base branch `main` moved to `newer-base-sha`")]
#[case::client_without_base_branch(None, "release branch `monochange/release/release` moved")]
#[tokio::test]
async fn release_commit_endpoint_rejects_moved_branches_as_conflicts(
	#[case] base_branch: Option<&str>,
	#[case] message: &str,
) {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	mock_branch(&github, "monochange/release/release", "newer-release-sha");
	mock_branch(&github, "main", "newer-base-sha");
	let mut request = request_with_path("crates/core/Cargo.toml");
	request.base_branch = base_branch.map(String::from);

	let (status, body) = post_release_commit(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::CONFLICT, "{body}");
	assert!(
		body["message"]
			.as_str()
			.is_some_and(|text| text.starts_with(message)),
		"{body}"
	);
}

#[rstest]
#[case::wrong_token(
	API_TOKEN,
	"not-the-api-token",
	StatusCode::UNAUTHORIZED,
	"invalid MONOCHANGE_TOKEN"
)]
#[case::unconfigured_token(
	"",
	API_TOKEN,
	StatusCode::SERVICE_UNAVAILABLE,
	"MONOCHANGE_TOKEN authentication is not configured on this deployment"
)]
#[tokio::test]
async fn release_commit_endpoint_requires_the_configured_api_token(
	#[case] configured: &str,
	#[case] bearer: &str,
	#[case] expected_status: StatusCode,
	#[case] message: &str,
) {
	let github = MockServer::start();
	let state = release_api_state(&github, configured).await;

	let (status, body) =
		post_release_commit(&state, bearer, &request_with_path("crates/core/Cargo.toml")).await;

	assert_eq!(status, expected_status, "{body}");
	assert_eq!(body["message"], message);
}

#[rstest]
#[case::nested_release_branch("monochange/release/release", Some("main"))]
#[case::slashed_base("monochange/release/release", Some("release/1.x"))]
#[case::no_base_branch("monochange/release/release", None)]
fn plausible_branch_names_are_accepted(#[case] branch: &str, #[case] base_branch: Option<&str>) {
	assert_eq!(validate_branch_names(branch, base_branch), Ok(()));
}

#[rstest]
#[case::empty("", None)]
#[case::parent_traversal("monochange/../main", None)]
#[case::leading_slash("/main", None)]
#[case::leading_dash("-main", None)]
#[case::control_character("main\n", None)]
#[case::url_fragment("main#x", None)]
#[case::url_query("main?ref=x", None)]
#[case::percent_encoding("monochange%2Frelease", None)]
#[case::trailing_slash("monochange/release/", None)]
#[case::lock_suffix("main.lock", None)]
#[case::double_slash("monochange//release", None)]
#[case::reflog_syntax("main@{1}", None)]
#[case::invalid_base_branch("monochange/release/release", Some("-main"))]
fn implausible_branch_names_are_rejected(#[case] branch: &str, #[case] base_branch: Option<&str>) {
	assert!(validate_branch_names(branch, base_branch).is_err());
}

#[tokio::test]
async fn release_commit_endpoint_rejects_invalid_branch_names() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	let mut request = request_with_path("crates/core/Cargo.toml");
	request.branch = "main#refs/heads/main".to_string();

	let (status, body) = post_release_commit(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
	assert_eq!(
		body["message"],
		"branch `main#refs/heads/main` is not a valid branch name"
	);
}

#[tokio::test]
async fn release_request_endpoint_rejects_invalid_branch_names() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	let payload = serde_json::json!({
		"request": {
			"provider": "github",
			"repository": "acme/actions",
			"owner": "acme",
			"repo": "actions",
			"base_branch": "main",
			"head_branch": "-release",
			"title": "chore(release): prepare release",
			"body": "",
			"labels": [],
			"auto_merge": false,
			"commit_message": { "subject": "chore(release): prepare release" },
		},
		"tracked_paths": [],
		"dry_run": false,
	});

	let response = api_router(state)
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/api/release-requests")
				.header("authorization", format!("Bearer {API_TOKEN}"))
				.header("content-type", "application/json")
				.body(Body::from(payload.to_string()))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(
		body["message"],
		"branch `-release` is not a valid branch name"
	);
}
