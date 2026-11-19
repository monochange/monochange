//! Hosted release RPC operations authenticate workflows before reading the
//! body, keep the REST routes' statuses, and answer JSON through the
//! production router.

#![cfg(not(target_arch = "wasm32"))]
// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::sync::OnceLock;

use axum::Router;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::header::CONTENT_TYPE;
use httpmock::Method;
use httpmock::MockServer;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::github_app::GitHubAppAuth;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;
use serde_json::Value;
use serde_json::json;
use server_fn::error::FromServerFnError;
use server_fn::error::ServerFnErrorErr;
use tower::ServiceExt;

use super::HostedRpcError;
use super::hosted_release_commit;
use super::is_machine_path;

const API_TOKEN: &str = "test-only-monochange-token";

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

/// `acme/actions` connected to installation 1001, the GitHub App pointed at
/// `github`, and [`API_TOKEN`] scoped to `acme/actions`.
async fn state(github: &MockServer) -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	for statement in [
		"INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'acme-admin', 'test-only-user-token')",
		"INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'acme', 'Organization')",
		"INSERT INTO repositories (id, installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 1, 11, 'acme/actions', 0)",
	] {
		sqlx::query(statement).execute(&db).await.unwrap();
	}
	let secrets: AppSecrets = serde_json::from_value(json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "hosted-rpc-test-signing-key",
		"token_encryption_key": "hosted-rpc-test-token-encryption-key",
		"github_client_id": "hosted-rpc-test-client",
		"github_client_secret": "hosted-rpc-test-client-secret",
		"monochange_token": API_TOKEN,
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_app = Some(GitHubAppAuth::new(
		"123",
		test_private_key(),
		"test-only-webhook-secret",
		&github.base_url(),
	));
	state.api_token_repositories = Some(std::iter::once("acme/actions".to_string()).collect());
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/app/installations/1001/access_tokens");
		then.status(201)
			.json_body(json!({ "token": "installation-token" }));
	});
	Arc::new(state)
}

fn production_router(state: &Arc<AppState>) -> Router {
	let options = leptos::config::LeptosOptions::builder()
		.output_name(monochange_shared::routing::WEBSITE_OUTPUT_NAME)
		.build();
	let rate_limit = crate::rate_limit::RateLimit::new(
		crate::rate_limit::RateLimitConfig::default(),
		state.jwt_secret.clone(),
		server_fn::axum::server_fn_paths().map(|(path, _)| path),
	);
	crate::router::build_router(state, options, rate_limit)
}

async fn post(
	app: &Router,
	path: &str,
	token: Option<&str>,
	body: impl Into<Body>,
) -> (StatusCode, Value) {
	let mut request = Request::post(path).header(CONTENT_TYPE, "application/json");
	if let Some(token) = token {
		request = request.header(AUTHORIZATION, format!("Bearer {token}"));
	}
	let response = app
		.clone()
		.oneshot(request.body(body.into()).unwrap())
		.await
		.unwrap();
	let status = response.status();
	let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
	(
		status,
		serde_json::from_slice(&bytes).unwrap_or(Value::Null),
	)
}

fn commit_request(provider: &str, dry_run: bool) -> Value {
	json!({
		"request": {
			"provider": provider,
			"owner": "acme",
			"repository": "actions",
			"branch": "monochange/release",
			"base_branch": "main",
			"base_commit": "base-sha",
			"subject": "chore(release): prepare release",
			"body": "",
			"files": [{ "path": "Cargo.toml", "content": "[package]\n" }],
			"dry_run": dry_run,
		}
	})
}

fn release_request(head_branch: &str) -> Value {
	json!({
		"request": {
			"request": {
				"provider": "github",
				"repository": "acme/actions",
				"owner": "acme",
				"repo": "actions",
				"base_branch": "main",
				"head_branch": head_branch,
				"title": "chore(release): prepare release",
				"body": "Release notes",
				"labels": ["release"],
				"auto_merge": false,
				"commit_message": { "subject": "chore(release): prepare release" },
			},
			"tracked_paths": ["Cargo.toml"],
			"dry_run": true,
		}
	})
}

fn dispatch_request(provider: &str) -> Value {
	json!({
		"request": {
			"provider": provider,
			"owner": "acme",
			"repository": "actions",
			"workflow": "release.yml",
			"ref": "main",
			"inputs": { "version": "1.2.3" },
			"dry_run": true,
		}
	})
}

#[tokio::test]
async fn hosted_operations_answer_json_for_a_scoped_api_token() {
	let github = MockServer::start();
	let app = production_router(&state(&github).await);

	let (status, commit) = post(
		&app,
		"/api/hosted_release_commit",
		Some(API_TOKEN),
		commit_request("github", true).to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::OK, "{commit}");
	assert_eq!(commit, json!({ "verified": false, "status": "dry_run" }));

	let (status, release) = post(
		&app,
		"/api/hosted_release_request",
		Some(API_TOKEN),
		release_request("monochange/release").to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::OK, "{release}");
	assert_eq!(
		release,
		json!({ "head_branch": "monochange/release", "message": "dry_run" })
	);

	let (status, dispatch) = post(
		&app,
		"/api/hosted_workflow_dispatch",
		Some(API_TOKEN),
		dispatch_request("github").to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::OK, "{dispatch}");
	assert_eq!(
		dispatch,
		json!({ "status": "dry_run", "workflow": "release.yml", "ref": "main" })
	);
}

#[tokio::test]
async fn hosted_operations_keep_the_rest_statuses() {
	let github = MockServer::start();
	let app = production_router(&state(&github).await);

	let (status, error) = post(
		&app,
		"/api/hosted_release_commit",
		Some(API_TOKEN),
		commit_request("gitlab", false).to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::BAD_REQUEST);
	assert_eq!(error["status"], 400);
	assert!(
		error["message"]
			.as_str()
			.unwrap()
			.contains("provider `gitlab` is not supported"),
		"{error}"
	);

	let (status, error) = post(
		&app,
		"/api/hosted_release_request",
		Some(API_TOKEN),
		release_request("bad..branch").to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");

	let (status, error) = post(
		&app,
		"/api/hosted_workflow_dispatch",
		Some(API_TOKEN),
		dispatch_request("gitlab").to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");

	let (status, error) = post(
		&app,
		"/api/hosted_release_commit",
		Some(API_TOKEN),
		"{not json",
	)
	.await;
	assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
	assert_eq!(error["status"], 400);
}

#[tokio::test]
async fn workflows_without_valid_credentials_are_refused_before_the_body() {
	let github = MockServer::start();
	let app = production_router(&state(&github).await);

	for path in [
		"/api/hosted_release_commit",
		"/api/hosted_release_request",
		"/api/hosted_workflow_dispatch",
	] {
		let (status, error) = post(&app, path, None, "{}").await;
		assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
		assert_eq!(error["status"], 401);
		assert!(
			error["message"]
				.as_str()
				.unwrap()
				.contains("Authorization: Bearer"),
			"{error}"
		);
	}

	let (status, error) = post(
		&app,
		"/api/hosted_release_commit",
		Some("wrong-token"),
		"{}",
	)
	.await;
	assert_eq!(status, StatusCode::UNAUTHORIZED);
	assert_eq!(error["message"], "invalid MONOCHANGE_TOKEN");
}

#[tokio::test]
async fn release_commits_accept_bodies_up_to_their_own_limit() {
	let github = MockServer::start();
	let app = production_router(&state(&github).await);

	// Larger than axum's 2 MiB default, inside the 8 MiB release commit limit.
	let mut request = commit_request("gitlab", false);
	request["request"]["body"] = Value::String("x".repeat(3 * 1024 * 1024));
	let (status, error) = post(
		&app,
		"/api/hosted_release_commit",
		Some(API_TOKEN),
		request.to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");

	request["request"]["body"] = Value::String("x".repeat(9 * 1024 * 1024));
	let (status, error) = post(
		&app,
		"/api/hosted_release_commit",
		Some(API_TOKEN),
		request.to_string(),
	)
	.await;
	assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
	assert_eq!(error["status"], 413);
}

#[tokio::test]
async fn an_operation_called_without_the_gate_is_unauthenticated() {
	let github = MockServer::start();
	let state = state(&github).await;
	let owner = Owner::new();
	let (parts, ()) = Request::new(()).into_parts();
	owner.with(|| {
		provide_context(state);
		provide_context(parts);
	});
	let request: monochange_core::HostedCommitRequest =
		serde_json::from_value(commit_request("github", true)["request"].clone()).unwrap();

	let error = owner
		.with(|| ScopedFuture::new(hosted_release_commit(request)))
		.await
		.unwrap_err();
	assert_eq!(error.status, 401);
}

#[test]
fn only_hosted_operations_are_machine_paths() {
	assert!(is_machine_path("/api/hosted_release_commit"));
	assert!(is_machine_path("/api/hosted_release_request"));
	assert!(is_machine_path("/api/hosted_workflow_dispatch"));
	assert!(!is_machine_path("/api/release-commits"));
	assert!(!is_machine_path("/api/get_session"));
}

#[test]
fn framework_failures_map_to_statuses() {
	let status = |error: ServerFnErrorErr| HostedRpcError::from_server_fn_error(error).status;
	assert_eq!(status(ServerFnErrorErr::Args("bad".into())), 400);
	assert_eq!(status(ServerFnErrorErr::MissingArg("request".into())), 400);
	assert_eq!(
		status(ServerFnErrorErr::Deserialization("bad json".into())),
		400
	);
	assert_eq!(
		status(ServerFnErrorErr::Deserialization(
			"request body exceeds the size limit".into()
		)),
		413
	);
	assert_eq!(status(ServerFnErrorErr::ServerError("boom".into())), 500);

	let unknown = HostedRpcError {
		status: 1000,
		message: "unknown".into(),
	};
	assert_eq!(unknown.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
	assert_eq!(unknown.to_string(), "HTTP 1000: unknown");
}
