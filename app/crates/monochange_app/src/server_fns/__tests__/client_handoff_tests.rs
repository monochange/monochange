//! Client sign-in RPC operations keep the handoff routes' code burning,
//! origin rule, body limit, and `no-store` answers through the production
//! router.

#![cfg(not(target_arch = "wasm32"))]
// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::AUTHORIZATION;
use axum::http::header::CACHE_CONTROL;
use axum::http::header::CONTENT_TYPE;
use axum::http::header::ORIGIN;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::client_auth::HandoffError;
use monochange_app_db::client_sessions;
use monochange_shared::code_challenge;
use serde_json::Value;
use serde_json::json;
use server_fn::error::FromServerFnError;
use server_fn::error::ServerFnErrorErr;
use tower::ServiceExt;

use super::HandoffRpcError;
use super::is_handoff_path;
use super::is_revoke_path;

const NATIVE_REDIRECT: &str = "dev.monochange.app:/auth/native/return";
const VERIFIER: &str = "known-verifier-known-verifier-known-verifier-known";
const TOKEN: &str = "/api/client_session_token";
const REVOKE: &str = "/api/client_session_revoke";

async fn state() -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	sqlx::query(
		"INSERT INTO users (id, github_id, github_login, github_access_token)
		 VALUES (1, 100, 'user-1', '')",
	)
	.execute(&db)
	.await
	.unwrap();
	let secrets: AppSecrets = serde_json::from_value(json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "client-handoff-rpc-test-signing-key",
		"token_encryption_key": "client-handoff-rpc-test-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.native_redirect_uris = vec![NATIVE_REDIRECT.to_string()];
	Arc::new(state)
}

async fn grant(state: &AppState) -> String {
	client_sessions::issue_grant(
		&state.db,
		1,
		"native",
		NATIVE_REDIRECT,
		&code_challenge(VERIFIER),
		0,
	)
	.await
	.unwrap()
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

fn exchange(code: &str) -> String {
	json!({
		"request": {
			"client": "native",
			"code": code,
			"code_verifier": VERIFIER,
			"redirect_uri": NATIVE_REDIRECT,
		}
	})
	.to_string()
}

struct Answer {
	status: StatusCode,
	no_store: bool,
	body: Value,
}

async fn post(
	app: &Router,
	path: &str,
	bearer: Option<&str>,
	origin: Option<&str>,
	body: impl Into<Body>,
) -> Answer {
	let mut request = Request::post(path).header(CONTENT_TYPE, "application/json");
	if let Some(bearer) = bearer {
		request = request.header(AUTHORIZATION, format!("Bearer {bearer}"));
	}
	if let Some(origin) = origin {
		request = request.header(ORIGIN, origin);
	}
	let response = app
		.clone()
		.oneshot(request.body(body.into()).unwrap())
		.await
		.unwrap();
	let status = response.status();
	let no_store = response
		.headers()
		.get(CACHE_CONTROL)
		.is_some_and(|value| value == "no-store");
	let bytes = to_bytes(response.into_body(), 1_000_000).await.unwrap();
	Answer {
		status,
		no_store,
		body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
	}
}

fn invalid() -> Value {
	json!({ "status": 400, "error": "invalid handoff request" })
}

#[tokio::test]
async fn a_client_exchanges_its_code_and_revokes_the_session_over_rpc() {
	let state = state().await;
	let app = production_router(&state);
	let code = grant(&state).await;

	let session = post(&app, TOKEN, None, None, exchange(&code)).await;
	assert_eq!(session.status, StatusCode::OK, "{}", session.body);
	assert!(session.no_store);
	assert_eq!(session.body["user"]["github_login"], "user-1");
	let bearer = session.body["access_token"].as_str().unwrap().to_owned();
	assert!(bearer.starts_with("mcd_"));

	// The code was burned by the exchange.
	let replay = post(&app, TOKEN, None, None, exchange(&code)).await;
	assert_eq!(replay.status, StatusCode::BAD_REQUEST);
	assert_eq!(replay.body, invalid());

	// A client session may call revoke, and only revoke.
	let token = post(&app, TOKEN, Some(&bearer), None, exchange(&code)).await;
	assert_eq!(token.status, StatusCode::FORBIDDEN);

	let revoked = post(&app, REVOKE, Some(&bearer), None, "{}").await;
	assert_eq!(revoked.status, StatusCode::OK, "{}", revoked.body);
	assert!(revoked.no_store);
	assert_eq!(revoked.body, Value::Null);

	// Revoking again reveals nothing about whether the session existed.
	let again = post(&app, REVOKE, Some(&bearer), None, "{}").await;
	assert_eq!(again.status, StatusCode::OK, "{}", again.body);
}

#[tokio::test]
async fn a_web_origin_cannot_burn_the_code() {
	let state = state().await;
	let app = production_router(&state);
	let code = grant(&state).await;

	let web = post(
		&app,
		TOKEN,
		None,
		Some("https://example.com"),
		exchange(&code),
	)
	.await;
	assert_eq!(web.status, StatusCode::BAD_REQUEST);
	assert!(web.no_store);
	assert_eq!(web.body, invalid());

	let extension = post(
		&app,
		TOKEN,
		None,
		Some("chrome-extension://abcdefghijklmnop"),
		exchange(&code),
	)
	.await;
	assert_eq!(extension.status, StatusCode::OK, "{}", extension.body);
}

#[tokio::test]
async fn sign_in_failures_keep_the_handoff_statuses() {
	let state = state().await;
	let app = production_router(&state);

	let unsigned = post(&app, REVOKE, None, None, "{}").await;
	assert_eq!(unsigned.status, StatusCode::UNAUTHORIZED);
	assert!(unsigned.no_store);
	assert_eq!(
		unsigned.body,
		json!({ "status": 401, "error": "authentication required" })
	);

	let malformed = post(&app, TOKEN, None, None, "{\"request\": 7}").await;
	assert_eq!(malformed.status, StatusCode::BAD_REQUEST);
	assert!(malformed.no_store);
	assert_eq!(malformed.body, invalid());

	let oversized = json!({ "request": { "code": "x".repeat(5000) } }).to_string();
	let oversized = post(&app, TOKEN, None, None, oversized).await;
	assert_eq!(oversized.status, StatusCode::PAYLOAD_TOO_LARGE);
	assert!(oversized.no_store);
}

#[test]
fn only_sign_in_operations_are_gated() {
	assert!(is_handoff_path(TOKEN));
	assert!(is_handoff_path(REVOKE));
	assert!(!is_handoff_path("/api/get_session"));
	assert!(is_revoke_path(REVOKE));
	assert!(!is_revoke_path(TOKEN));
}

#[test]
fn handoff_errors_map_to_rpc_errors() {
	let error = HandoffRpcError::from(HandoffError::new(StatusCode::FORBIDDEN, "forbidden"));
	assert_eq!(error.status_code(), StatusCode::FORBIDDEN);
	assert_eq!(error.to_string(), "HTTP 403: forbidden");

	let framework = HandoffRpcError::from_server_fn_error(ServerFnErrorErr::ServerError(
		"context missing".into(),
	));
	assert_eq!(framework.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
	assert_eq!(framework.error, "handoff unavailable");

	let unknown = HandoffRpcError {
		status: 1000,
		error: "?".into(),
	};
	assert_eq!(unknown.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
}
