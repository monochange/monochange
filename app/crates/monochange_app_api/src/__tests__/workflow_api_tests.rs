//! Hosted workflow dispatch validation, authorization, and responses.

// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use httpmock::Method;
use httpmock::MockServer;
use monochange_core::HostedWorkflowDispatchRequest;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;
use rstest::rstest;
use tower::ServiceExt;

use super::authorize_workflow_dispatch;
use super::validate_workflow_dispatch_request;
use crate::AppSecrets;
use crate::AppState;
use crate::api_router;
use crate::github_app::GitHubAppAuth;
use crate::release_api::Caller;

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

fn request(repository: &str) -> HostedWorkflowDispatchRequest {
	HostedWorkflowDispatchRequest {
		provider: "github".to_string(),
		owner: "acme".to_string(),
		repository: repository.to_string(),
		workflow: "release.yml".to_string(),
		git_ref: "main".to_string(),
		inputs: BTreeMap::new(),
		dry_run: false,
	}
}

async fn state(github: &MockServer) -> AppState {
	state_with_token(
		github,
		201,
		serde_json::json!({ "token": "installation-token" }),
	)
	.await
}

async fn state_with_token(
	github: &MockServer,
	token_status: u16,
	token_body: serde_json::Value,
) -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	for statement in [
		"INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'acme-admin', 'test-only-user-token')",
		"INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'acme', 'Organization')",
		"INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (2, 1, 2001, 'other', 'Organization')",
		"INSERT INTO repositories (id, installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 1, 10, 'acme/monochange', 0)",
		"INSERT INTO repositories (id, installation_id, github_repo_id, github_full_name, github_private) VALUES (2, 1, 11, 'acme/actions', 0)",
		"INSERT INTO repositories (id, installation_id, github_repo_id, github_full_name, github_private) VALUES (3, 2, 21, 'other/actions', 0)",
	] {
		sqlx::query(statement).execute(&db).await.unwrap();
	}
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "workflow-api-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
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
		then.status(token_status).json_body(token_body);
	});
	state
}

async fn post_dispatch(
	state: &AppState,
	bearer: &str,
	request: &HostedWorkflowDispatchRequest,
) -> (StatusCode, serde_json::Value) {
	let response = api_router(state.clone())
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/api/workflow-dispatches")
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

#[rstest]
#[case::invalid_provider(
	"gitlab",
	"acme",
	"actions",
	"release.yml",
	"main",
	"provider `gitlab` is not supported"
)]
#[case::invalid_owner(
	"github",
	"acme/other",
	"actions",
	"release.yml",
	"main",
	"owner `acme/other` is not a valid"
)]
#[case::invalid_repository(
	"github",
	"acme",
	"../actions",
	"release.yml",
	"main",
	"repository `../actions` is not a valid"
)]
#[case::invalid_workflow(
	"github",
	"acme",
	"actions",
	"../release.yml",
	"main",
	"must be a `.yml` or `.yaml` file name"
)]
#[case::invalid_ref(
	"github",
	"acme",
	"actions",
	"release.yml",
	"main#x",
	"ref `main#x` is not a valid branch name"
)]
fn malformed_dispatch_requests_are_rejected(
	#[case] provider: &str,
	#[case] owner: &str,
	#[case] repository: &str,
	#[case] workflow: &str,
	#[case] git_ref: &str,
	#[case] expected: &str,
) {
	let request = HostedWorkflowDispatchRequest {
		provider: provider.to_string(),
		owner: owner.to_string(),
		repository: repository.to_string(),
		workflow: workflow.to_string(),
		git_ref: git_ref.to_string(),
		inputs: BTreeMap::new(),
		dry_run: false,
	};

	let error = validate_workflow_dispatch_request(&request).expect_err("request must fail");
	assert!(error.contains(expected), "{error}");
}

#[test]
fn numeric_workflow_ids_and_valid_inputs_are_accepted() {
	let mut request = request("actions");
	request.workflow = "42".to_string();
	request.inputs = BTreeMap::from([
		("pull_request".to_string(), "103".to_string()),
		("comment".to_string(), "always".to_string()),
	]);

	assert_eq!(validate_workflow_dispatch_request(&request), Ok(()));

	request.workflow = "RELEASE.YAML".to_string();
	assert_eq!(validate_workflow_dispatch_request(&request), Ok(()));
}

#[test]
fn empty_and_overlong_workflows_are_rejected() {
	for workflow in [String::new(), "a".repeat(256)] {
		let mut request = request("actions");
		request.workflow = workflow;
		assert!(
			validate_workflow_dispatch_request(&request)
				.expect_err("invalid workflow length must fail")
				.contains("file name or numeric workflow id")
		);
	}
}

#[test]
fn workflow_dispatch_rejects_too_many_or_oversized_inputs() {
	let mut too_many = request("actions");
	too_many.inputs = (0..11)
		.map(|index| (format!("field_{index}"), "value".to_string()))
		.collect();
	assert!(
		validate_workflow_dispatch_request(&too_many)
			.expect_err("11 inputs must fail")
			.contains("at most 10 inputs")
	);

	let mut oversized = request("actions");
	oversized
		.inputs
		.insert("body".to_string(), "x".repeat(1025));
	assert!(
		validate_workflow_dispatch_request(&oversized)
			.expect_err("oversized value must fail")
			.contains("maximum is 1024")
	);

	let mut invalid_name = request("actions");
	invalid_name
		.inputs
		.insert("bad name".to_string(), "value".to_string());
	assert!(
		validate_workflow_dispatch_request(&invalid_name)
			.expect_err("invalid input name must fail")
			.contains("is not valid")
	);
}

#[tokio::test]
async fn oidc_callers_may_dispatch_within_the_same_installation() {
	let github = MockServer::start();
	let state = state(&github).await;
	let caller = Caller::ActionsRun {
		repository: "acme/monochange".to_string(),
	};

	let target = authorize_workflow_dispatch(&state, &caller, "acme/actions")
		.await
		.expect("same-installation dispatch should be allowed");
	assert_eq!(target.installation_github_id, 1001);
}

#[tokio::test]
async fn oidc_callers_may_not_dispatch_across_installations() {
	let github = MockServer::start();
	let state = state(&github).await;
	let caller = Caller::ActionsRun {
		repository: "acme/monochange".to_string(),
	};

	let (status, body) = authorize_workflow_dispatch(&state, &caller, "other/actions")
		.await
		.expect_err("cross-installation dispatch must fail");
	assert_eq!(status, StatusCode::FORBIDDEN);
	assert_eq!(
		body.0.message,
		"OIDC token repository `acme/monochange` and target `other/actions` are not in the same monochange GitHub App installation"
	);
}

#[tokio::test]
async fn oidc_caller_without_an_installation_is_forbidden() {
	let github = MockServer::start();
	let state = state(&github).await;
	let caller = Caller::ActionsRun {
		repository: "missing/repository".to_string(),
	};

	let (status, body) = authorize_workflow_dispatch(&state, &caller, "acme/actions")
		.await
		.expect_err("unconnected caller must fail");
	assert_eq!(status, StatusCode::FORBIDDEN);
	assert_eq!(
		body.0.message,
		"OIDC token repository `missing/repository` has no monochange GitHub App installation"
	);
}

#[tokio::test]
async fn repository_lookup_failures_are_internal_server_errors() {
	let github = MockServer::start();
	let state = state(&github).await;
	state.db.close().await;

	let (status, body) = authorize_workflow_dispatch(&state, &Caller::ApiToken, "acme/actions")
		.await
		.expect_err("closed database must fail");
	assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
	assert!(
		body.0.message.contains("repository lookup failed"),
		"{}",
		body.0.message
	);
}

#[tokio::test]
async fn api_tokens_may_only_dispatch_to_scoped_repositories() {
	let github = MockServer::start();
	let state = state(&github).await;

	let target = authorize_workflow_dispatch(&state, &Caller::ApiToken, "acme/actions")
		.await
		.expect("scoped target should be allowed");
	assert_eq!(target.github_repo_id, 11);

	let (status, body) = authorize_workflow_dispatch(&state, &Caller::ApiToken, "acme/monochange")
		.await
		.expect_err("unscoped target must fail");
	assert_eq!(status, StatusCode::FORBIDDEN);
	assert_eq!(
		body.0.message,
		"MONOCHANGE_TOKEN is not scoped to `acme/monochange`; add the repository to MONOCHANGE_TOKEN_REPOSITORIES or use a GitHub Actions OIDC token"
	);
}

#[tokio::test]
async fn missing_installation_is_not_found() {
	let github = MockServer::start();
	let state = state(&github).await;

	let (status, body) = authorize_workflow_dispatch(&state, &Caller::ApiToken, "acme/missing")
		.await
		.expect_err("unconnected target must fail");
	assert_eq!(status, StatusCode::NOT_FOUND);
	assert_eq!(
		body.0.message,
		"repository `acme/missing` has no monochange GitHub App installation"
	);
}

#[tokio::test]
async fn workflow_dispatch_endpoint_dispatches_and_passes_inputs() {
	let github = MockServer::start();
	let state = state(&github).await;
	let dispatch = github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/actions/workflows/release.yml/dispatches")
			.header("authorization", "Bearer installation-token")
			.json_body(serde_json::json!({
				"ref": "main",
				"inputs": {"pull_request": "103"}
			}));
		then.status(204);
	});
	let mut request = request("actions");
	request
		.inputs
		.insert("pull_request".to_string(), "103".to_string());

	let (status, body) = post_dispatch(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::OK, "{body}");
	assert_eq!(body["status"], "dispatched");
	assert_eq!(body["workflow"], "release.yml");
	assert_eq!(body["ref"], "main");
	dispatch.assert_calls(1);
}

#[tokio::test]
async fn dry_run_validates_without_dispatching() {
	let github = MockServer::start();
	let state = state(&github).await;
	let dispatch = github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/actions/workflows/release.yml/dispatches");
		then.status(204);
	});
	let mut request = request("actions");
	request.dry_run = true;

	let (status, body) = post_dispatch(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::OK, "{body}");
	assert_eq!(body["status"], "dry_run");
	dispatch.assert_calls(0);
}

#[tokio::test]
async fn github_rejections_preserve_actionable_statuses() {
	let github = MockServer::start();
	let state = state(&github).await;
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/actions/workflows/missing.yml/dispatches");
		then.status(404)
			.json_body(serde_json::json!({"message": "Not Found"}));
	});
	let mut request = request("actions");
	request.workflow = "missing.yml".to_string();

	let (status, body) = post_dispatch(&state, API_TOKEN, &request).await;

	assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
	assert_eq!(
		body["message"],
		"GitHub API `dispatch workflow` failed with status 404 Not Found: {\"message\":\"Not Found\"}"
	);
}

#[tokio::test]
async fn workflow_rejections_cover_unprocessable_and_server_errors() {
	let github = MockServer::start();
	let state = state(&github).await;
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/actions/workflows/invalid.yml/dispatches");
		then.status(422)
			.json_body(serde_json::json!({"message": "Invalid request"}));
	});
	github.mock(|when, then| {
		when.method(Method::POST)
			.path("/repos/acme/actions/actions/workflows/broken.yml/dispatches");
		then.status(500)
			.json_body(serde_json::json!({"message": "Server error"}));
	});

	let mut invalid = request("actions");
	invalid.workflow = "invalid.yml".to_string();
	let (status, body) = post_dispatch(&state, API_TOKEN, &invalid).await;
	assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");

	let mut broken = request("actions");
	broken.workflow = "broken.yml".to_string();
	let (status, body) = post_dispatch(&state, API_TOKEN, &broken).await;
	assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
	assert!(
		body["message"]
			.as_str()
			.is_some_and(|message| message.contains("status 500")),
		"{body}"
	);
}

#[tokio::test]
async fn installation_token_failures_are_bad_gateway() {
	let github = MockServer::start();
	let state = state_with_token(
		&github,
		500,
		serde_json::json!({"message": "installation token unavailable"}),
	)
	.await;

	let (status, body) = post_dispatch(&state, API_TOKEN, &request("actions")).await;

	assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
	assert!(
		body["message"]
			.as_str()
			.is_some_and(|message| message.contains("failed to mint installation token")),
		"{body}"
	);
}

#[tokio::test]
async fn unconfigured_app_returns_service_unavailable() {
	let github = MockServer::start();
	let mut state = state(&github).await;
	state.github_app = None;

	let (status, body) = post_dispatch(&state, API_TOKEN, &request("actions")).await;

	assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
	assert_eq!(
		body["message"],
		"the monochange GitHub App is not configured on this deployment"
	);
}
