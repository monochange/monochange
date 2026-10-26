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

use super::Caller;
use super::authorize;
use super::commit_release_files;
use super::release_commit_error_status;
use super::validate_branch_names;
use super::validate_commit_contents;
use super::validate_commit_paths;
use crate::AppSecrets;
use crate::AppState;
use crate::RELEASE_COMMIT_BODY_LIMIT;
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
		tree: None,
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
		release_commit_error_status(&GitHubAppError::UnverifiedCommit(
			"release-sha".to_string(),
			"unsigned".to_string(),
		)),
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
	let server =
		github_creating_release_branch(&serde_json::json!({ "verified": true, "reason": "valid" }));

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
			verified: true,
			status: Some("completed".to_string()),
			message: Some("valid".to_string()),
		}
	);
}

#[tokio::test]
async fn unverified_release_commit_is_a_bad_gateway() {
	let server = github_creating_release_branch(
		&serde_json::json!({ "verified": false, "reason": "unsigned" }),
	);

	let (status, body) = commit_release_files(
		&reqwest::Client::new(),
		&server.base_url(),
		"installation-token",
		&request_with_path("crates/core/Cargo.toml"),
	)
	.await
	.unwrap_err();

	assert_eq!(status, StatusCode::BAD_GATEWAY);
	assert_eq!(
		body.0.message,
		"GitHub created the release commit `release-sha` but reported it as unverified (unsigned); the release branch was not moved"
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
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "release-api-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
		"monochange_token": api_token,
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_app = Some(GitHubAppAuth::new(
		"123",
		test_private_key(),
		"test-only-webhook-secret",
		&github.base_url(),
	));
	// The API token may only act on the repositories a deployment lists in
	// MONOCHANGE_TOKEN_REPOSITORIES; the harness scopes it to `acme/actions`.
	state.api_token_repositories = Some(
		["acme/actions"]
			.iter()
			.map(|repository| (*repository).to_string())
			.collect(),
	);
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/app/installations/1001/access_tokens")
			.header("user-agent", "monochange");
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
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "release-api-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
		"monochange_token": "",
	}))
	.unwrap();
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	assert_eq!(AppState::new(db, secrets).unwrap().api_token, None);
}

#[tokio::test]
async fn api_token_cannot_act_outside_its_configured_repositories() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;

	let mut request = request_with_path(".monochange/releases/829c44b7a1ee8674/release.json");
	request.repository = "unscoped-repository".to_string();

	let (status, body) = post_release_commit(&state, API_TOKEN, &request).await;
	assert_eq!(status, StatusCode::FORBIDDEN);
	assert!(
		body["message"]
			.as_str()
			.unwrap()
			.contains("MONOCHANGE_TOKEN is not scoped to `acme/unscoped-repository`"),
		"body: {body}"
	);
}

#[tokio::test]
async fn api_token_scope_matches_repository_names_case_insensitively() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;

	// `Acme/Actions` lowercases onto the scoped `acme/actions`, so
	// authorization passes; the exact-case repository lookup then misses and
	// the request ends as a 404 rather than a 403 scope rejection.
	let mut request = request_with_path(".monochange/releases/829c44b7a1ee8674/release.json");
	request.owner = "Acme".to_string();
	request.repository = "Actions".to_string();

	let (status, _) = post_release_commit(&state, API_TOKEN, &request).await;
	assert_ne!(
		status,
		StatusCode::FORBIDDEN,
		"case-insensitive scope must authorize"
	);
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
			.path("/repos/acme/actions/git/commits")
			.header("user-agent", "monochange");
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

/// Mock `GET git/trees/{tree}` listing the given `(path, sha)` entries.
fn mock_tree_listing(github: &MockServer, tree: &str, entries: &[(&str, &str)]) {
	let entries = entries
		.iter()
		.map(
			|(path, sha)| serde_json::json!({ "path": path, "mode": "040000", "type": "tree", "sha": sha }),
		)
		.collect::<Vec<_>>();
	github.mock(|when, then| {
		when.method(Method::GET)
			.path(format!("/repos/acme/actions/git/trees/{tree}"));
		then.status(200)
			.json_body(serde_json::json!({ "sha": tree, "tree": entries, "truncated": false }));
	});
}

/// Trees on either side of a workflow-built release commit: the base tree
/// and its `.github`/`workflows` entries, then the same for the release tree.
struct WorkflowTrees {
	base: &'static str,
	base_github: Option<&'static str>,
	base_workflows: Option<&'static str>,
	release_github: Option<&'static str>,
	release_workflows: Option<&'static str>,
}

fn mock_workflow_trees(github: &MockServer, trees: &WorkflowTrees) {
	github.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/commits/base-sha");
		then.status(200).json_body(serde_json::json!({
			"sha": "base-sha",
			"tree": { "sha": trees.base },
		}));
	});
	let listing = |github_entry: Option<&'static str>| {
		github_entry.map_or_else(Vec::new, |sha| vec![(".github", sha), ("src", "src-tree")])
	};
	mock_tree_listing(github, trees.base, &listing(trees.base_github));
	mock_tree_listing(github, WORKFLOW_TREE, &listing(trees.release_github));
	for (github_tree, workflows) in [
		(trees.base_github, trees.base_workflows),
		(trees.release_github, trees.release_workflows),
	] {
		if let Some(github_tree) = github_tree {
			let entries = workflows.map_or_else(Vec::new, |sha| vec![("workflows", sha)]);
			mock_tree_listing(github, github_tree, &entries);
		}
	}
}

#[rstest]
#[case::same_tree(WorkflowTrees {
	base: WORKFLOW_TREE,
	base_github: None,
	base_workflows: None,
	release_github: None,
	release_workflows: None,
}, StatusCode::OK)]
#[case::same_github_directory(WorkflowTrees {
	base: "base-tree",
	base_github: Some("github-tree"),
	base_workflows: Some("workflows-tree"),
	release_github: Some("github-tree"),
	release_workflows: Some("workflows-tree"),
}, StatusCode::OK)]
#[case::other_github_files_changed(WorkflowTrees {
	base: "base-tree",
	base_github: Some("github-tree"),
	base_workflows: Some("workflows-tree"),
	release_github: Some("release-github-tree"),
	release_workflows: Some("workflows-tree"),
}, StatusCode::OK)]
#[case::workflows_changed(WorkflowTrees {
	base: "base-tree",
	base_github: Some("github-tree"),
	base_workflows: Some("workflows-tree"),
	release_github: Some("release-github-tree"),
	release_workflows: Some("release-workflows-tree"),
}, StatusCode::UNPROCESSABLE_ENTITY)]
#[case::github_directory_removed(WorkflowTrees {
	base: "base-tree",
	base_github: Some("github-tree"),
	base_workflows: Some("workflows-tree"),
	release_github: None,
	release_workflows: None,
}, StatusCode::UNPROCESSABLE_ENTITY)]
#[tokio::test]
async fn release_commit_endpoint_commits_a_tree_built_by_the_workflow(
	#[case] trees: WorkflowTrees,
	#[case] expected: StatusCode,
) {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	mock_branch(&github, "main", "base-sha");
	github.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/ref/heads/monochange/release/release");
		then.status(404);
	});
	mock_workflow_trees(&github, &trees);
	let blobs = github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/blobs");
		then.status(201)
			.json_body(serde_json::json!({ "sha": "blob-sha" }));
	});
	let commit = github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/commits")
			.json_body_includes(serde_json::json!({ "tree": WORKFLOW_TREE }).to_string());
		then.status(201).json_body(serde_json::json!({
			"sha": "release-sha",
			"verification": { "verified": true, "reason": "valid" },
		}));
	});
	let create = github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/git/refs");
		then.status(201)
			.json_body(serde_json::json!({ "object": { "sha": "release-sha" } }));
	});
	let mut request = request_with_path("crates/core/Cargo.toml");
	request.files.clear();
	request.tree = Some(WORKFLOW_TREE.to_string());

	let (status, body) = post_release_commit(&state, API_TOKEN, &request).await;

	assert_eq!(status, expected, "{body}");
	blobs.assert_calls(0);
	if expected == StatusCode::OK {
		assert_eq!(body["commit"], "release-sha");
		commit.assert();
		create.assert();
	} else {
		assert_eq!(
			body["message"],
			"the release tree changes `.github/workflows`; release commits must not modify GitHub workflows"
		);
		commit.assert_calls(0);
		create.assert_calls(0);
	}
}

#[tokio::test]
async fn release_commit_endpoint_reports_unreadable_workflow_trees() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	mock_branch(&github, "main", "base-sha");
	github.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/ref/heads/monochange/release/release");
		then.status(404);
	});
	github.mock(|when, then| {
		when.method(Method::GET)
			.path("/repos/acme/actions/git/commits/base-sha");
		then.status(404).body("Not Found");
	});
	let mut request = request_with_path("crates/core/Cargo.toml");
	request.files.clear();
	request.tree = Some(WORKFLOW_TREE.to_string());

	let (status, body) = post_release_commit(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
	assert!(
		body["message"]
			.as_str()
			.is_some_and(|message| message.contains("read base commit")),
		"{body}"
	);
}

const WORKFLOW_TREE: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

#[rstest]
#[case::files_only(false, None, Ok(()))]
#[case::tree_only(true, Some(WORKFLOW_TREE), Ok(()))]
#[case::both(
	false,
	Some(WORKFLOW_TREE),
	Err("send either release `files` or a `tree`, not both")
)]
#[case::neither(true, None, Err("the release commit has no files and no `tree`"))]
#[case::not_an_object_id(
	true,
	Some("main"),
	Err("`tree` must be a git object id, received `main`")
)]
#[case::not_hex(
	true,
	Some("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
	Err("`tree` must be a git object id, received `zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz`")
)]
fn release_commits_need_exactly_one_tree_source(
	#[case] clear_files: bool,
	#[case] tree: Option<&str>,
	#[case] expected: Result<(), &str>,
) {
	let mut request = request_with_path("crates/core/Cargo.toml");
	if clear_files {
		request.files.clear();
	}
	request.tree = tree.map(String::from);

	assert_eq!(
		validate_commit_contents(&request),
		expected.map_err(String::from)
	);
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

#[tokio::test]
async fn release_commit_endpoint_reads_bodies_larger_than_the_axum_default() {
	// A grouped release rewrites every changelog; monochange's own already
	// exceed axum's 2 MiB default.
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	let mut request = request_with_path("crates/core/CHANGELOG.md");
	request.files[0].content = Some("- release note\n".repeat(256 * 1024));
	request.dry_run = true;

	let (status, body) = post_release_commit(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::OK, "{body}");
	assert_eq!(body["status"], "dry_run");
}

async fn post_raw_release_commit(
	state: &AppState,
	authorization: Option<&str>,
	body: Vec<u8>,
) -> (StatusCode, serde_json::Value) {
	let mut builder = Request::builder()
		.method("POST")
		.uri("/api/release-commits")
		.header("content-type", "application/json");
	if let Some(authorization) = authorization {
		builder = builder.header("authorization", authorization);
	}
	let response = api_router(state.clone())
		.oneshot(builder.body(Body::from(body)).unwrap())
		.await
		.unwrap();
	let status = response.status();
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	(status, serde_json::from_slice(&body).unwrap())
}

#[rstest]
#[case::over_the_limit(
	Some("Bearer test-only-api-token"),
	vec![b' '; RELEASE_COMMIT_BODY_LIMIT + 1],
	StatusCode::PAYLOAD_TOO_LARGE,
	"request body is larger than the 8388608-byte limit or could not be read"
)]
#[case::not_json(
	Some("Bearer test-only-api-token"),
	b"not json".to_vec(),
	StatusCode::BAD_REQUEST,
	"invalid request body: expected ident at line 1 column 2"
)]
#[case::unauthenticated_over_the_limit(
	Some("Bearer not-the-api-token"),
	vec![b' '; RELEASE_COMMIT_BODY_LIMIT + 1],
	StatusCode::UNAUTHORIZED,
	"invalid MONOCHANGE_TOKEN"
)]
#[case::missing_authorization(
	None,
	vec![b' '; RELEASE_COMMIT_BODY_LIMIT + 1],
	StatusCode::UNAUTHORIZED,
	"set an Authorization: Bearer header to a GitHub Actions OIDC token or MONOCHANGE_TOKEN"
)]
#[case::malformed_oidc_token(
	Some("Bearer not.a.jwt"),
	vec![b' '; RELEASE_COMMIT_BODY_LIMIT + 1],
	StatusCode::UNAUTHORIZED,
	"OIDC verification failed: OIDC token is missing or malformed: Base64 error: Invalid last symbol 116, offset 2."
)]
#[tokio::test]
async fn release_commit_endpoint_checks_credentials_before_the_body(
	#[case] authorization: Option<&str>,
	#[case] body: Vec<u8>,
	#[case] expected_status: StatusCode,
	#[case] message: &str,
) {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;

	let (status, body) = post_raw_release_commit(&state, authorization, body).await;

	assert_eq!(status, expected_status, "{body}");
	assert_eq!(body["message"], message);
}

#[rstest]
#[case::matching_oidc_repository("acme/actions", true)]
#[case::other_oidc_repository("acme/other", false)]
#[tokio::test]
async fn oidc_callers_may_only_act_on_their_own_repository(
	#[case] claimed: &str,
	#[case] allowed: bool,
) {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;
	let caller = Caller::ActionsRun {
		repository: claimed.to_string(),
	};

	let result = authorize(&state, &caller, "acme/actions");

	assert_eq!(result.is_ok(), allowed);
	if let Err((status, body)) = result {
		assert_eq!(status, StatusCode::FORBIDDEN);
		assert_eq!(
			body.0.message,
			"OIDC token is for repository `acme/other` but the request targets `acme/actions`"
		);
	}
}

#[tokio::test]
async fn api_token_callers_may_act_only_on_scoped_repositories() {
	let github = MockServer::start();
	let state = release_api_state(&github, API_TOKEN).await;

	assert!(authorize(&state, &Caller::ApiToken, "acme/actions").is_ok());

	let Err((status, body)) = authorize(&state, &Caller::ApiToken, "acme/other") else {
		panic!("an unscoped repository must reject the API token");
	};
	assert_eq!(status, StatusCode::FORBIDDEN);
	assert_eq!(
		body.0.message,
		"MONOCHANGE_TOKEN is not scoped to `acme/other`; add the repository to MONOCHANGE_TOKEN_REPOSITORIES or use a GitHub Actions OIDC token"
	);
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
