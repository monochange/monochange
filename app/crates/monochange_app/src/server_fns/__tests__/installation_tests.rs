//! Repository setup requires a real session and a verified GitHub App identity.

#![cfg(not(target_arch = "wasm32"))]
// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::sync::OnceLock;

use axum::http::Request;
use axum::http::header::COOKIE;
use httpmock::Method;
use httpmock::MockServer;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::create_token;
use monochange_app_api::github_app::GitHubAppAuth;
use monochange_app_api::oauth;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;

use super::RepositoryConnectionStatus;
use super::repository_connection;

async fn state(server: Option<&MockServer>) -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token')")
		.execute(&db)
		.await
		.unwrap();
	let secrets: AppSecrets =
		serde_json::from_value(serde_json::json!({"jwt_secret": "connection-test-signing-key"}))
			.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();

	if let Some(server) = server {
		static TEST_KEY: OnceLock<String> = OnceLock::new();
		let pem = TEST_KEY.get_or_init(|| {
			RsaPrivateKey::new(&mut OsRng, 2048)
				.unwrap()
				.to_pkcs1_pem(LineEnding::LF)
				.unwrap()
				.to_string()
		});
		state.github_app = Some(GitHubAppAuth::new(
			"123",
			pem,
			"test-only-webhook-secret",
			&server.base_url(),
		));
	}

	Arc::new(state)
}

fn context(state: Arc<AppState>, cookie: Option<String>) -> Owner {
	let owner = Owner::new();
	let mut request = Request::new(());

	if let Some(cookie) = cookie {
		request
			.headers_mut()
			.insert(COOKIE, cookie.parse().unwrap());
	}

	let (parts, ()) = request.into_parts();
	owner.with(|| {
		provide_context(state);
		provide_context(parts);
	});
	owner
}

fn session(state: &AppState, user_id: i32) -> String {
	let token = create_token(&state.jwt_secret, user_id, 101, "alice").unwrap();
	format!("{}={token}", oauth::SESSION_COOKIE_NAME)
}

#[tokio::test]
async fn signed_out_visitor_does_not_lookup_or_receive_an_installation_link() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200)
			.json_body(serde_json::json!({"slug": "test-app"}));
	});
	let owner = context(state(Some(&server)).await, None);

	assert_eq!(
		owner
			.with(|| ScopedFuture::new(repository_connection()))
			.await
			.unwrap(),
		RepositoryConnectionStatus::SignedOut,
	);
	metadata.assert_calls(0);
}

#[tokio::test]
async fn signed_in_visitor_can_open_the_configured_app_installation_page() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/app")
			.header_exists("authorization");
		then.status(200)
			.json_body(serde_json::json!({"slug": "test-app"}));
	});
	let state = state(Some(&server)).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));

	assert_eq!(
		owner
			.with(|| ScopedFuture::new(repository_connection()))
			.await
			.unwrap(),
		RepositoryConnectionStatus::Available {
			installation_url: "https://github.com/apps/test-app/installations/new".to_string()
		},
	);
	metadata.assert();
}

#[tokio::test]
async fn unconfigured_app_is_explicitly_unavailable_to_a_signed_in_visitor() {
	let state = state(None).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));

	assert_eq!(
		owner
			.with(|| ScopedFuture::new(repository_connection()))
			.await
			.unwrap(),
		RepositoryConnectionStatus::Unavailable,
	);
}

#[tokio::test]
async fn configured_app_request_failure_is_an_error_and_does_not_leak_response_body() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(503)
			.json_body(serde_json::json!({"message": "test-only-sensitive-response"}));
	});
	let state = state(Some(&server)).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let error = owner
		.with(|| ScopedFuture::new(repository_connection()))
		.await
		.unwrap_err()
		.to_string();

	assert!(error.contains("503"));
	assert!(!error.contains("test-only-sensitive-response"));
	metadata.assert();
}

#[tokio::test]
async fn invalid_session_cannot_request_app_metadata() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200)
			.json_body(serde_json::json!({"slug": "test-app"}));
	});
	let state = state(Some(&server)).await;
	let owner = context(
		state,
		Some(format!("{}=invalid-token", oauth::SESSION_COOKIE_NAME)),
	);

	assert!(
		owner
			.with(|| ScopedFuture::new(repository_connection()))
			.await
			.is_err()
	);
	metadata.assert_calls(0);
}

#[tokio::test]
async fn session_for_deleted_user_cannot_receive_an_installation_link() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200)
			.json_body(serde_json::json!({"slug": "test-app"}));
	});
	let state = state(Some(&server)).await;
	let cookie = session(&state, 999);
	let owner = context(state, Some(cookie));

	assert_eq!(
		owner
			.with(|| ScopedFuture::new(repository_connection()))
			.await
			.unwrap(),
		RepositoryConnectionStatus::SignedOut
	);
	metadata.assert_calls(0);
}

#[tokio::test]
async fn invalid_app_metadata_is_an_error_for_the_dashboard() {
	let server = MockServer::start();
	let metadata = server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(200)
			.json_body(serde_json::json!({"slug": "../another-app"}));
	});
	let state = state(Some(&server)).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let error = owner
		.with(|| ScopedFuture::new(repository_connection()))
		.await
		.unwrap_err()
		.to_string();

	assert!(error.contains("could not be loaded"));
	metadata.assert();
}
