//! Tests of the server functions' OAuth and response-cookie wiring.

#![cfg(not(target_arch = "wasm32"))]
// The tokio test macro constructs its runtime with `expect`.
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::http::Request;
use axum::http::header::COOKIE;
use axum::http::header::SET_COOKIE;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::CookieJar;
use httpmock::Method::GET;
use httpmock::Method::POST;
use httpmock::MockServer;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use leptos_axum::ResponseOptions;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::oauth;

use super::GitHubUserTokenResponse;
use super::exchange_code;
use super::get_login_url;
use super::get_session;
use super::github_user_access_token;
use super::logout;

async fn state() -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"jwt_secret": "server-function-test-signing-key",
		"github_client_id": "client-test-id",
		"github_client_secret": "client-test-secret",
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	// Any accidental exchange must fail locally instead of contacting GitHub.
	state.http = reqwest::Client::builder()
		.proxy(reqwest::Proxy::all("http://127.0.0.1:9").unwrap())
		.build()
		.unwrap();
	Arc::new(state)
}

async fn state_with_server(server: &MockServer) -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"jwt_secret": "server-function-test-signing-key",
		"github_client_id": "client-test-id",
		"github_client_secret": "client-test-secret",
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_oauth_origin = server.base_url();
	state.github_api_origin = server.base_url();
	Arc::new(state)
}

async fn insert_expired_refreshable_user(state: &AppState) {
	let now = chrono::Utc::now().timestamp();
	sqlx::query(
		"INSERT INTO users (
			id, github_id, github_login, github_access_token, github_refresh_token,
			github_access_token_expires_at, github_refresh_token_expires_at
		 ) VALUES (1, 101, 'alice', 'old-access', 'old-refresh', $1, $2)",
	)
	.bind(now - 1)
	.bind(now + 3600)
	.execute(&state.db)
	.await
	.unwrap();
}

fn context(state: Arc<AppState>, cookie_header: Option<String>) -> (Owner, ResponseOptions) {
	let owner = Owner::new();
	let response = ResponseOptions::default();
	let mut request = Request::new(());
	if let Some(cookie) = cookie_header {
		request
			.headers_mut()
			.insert(COOKIE, cookie.parse().unwrap());
	}
	let (parts, ()) = request.into_parts();
	owner.with(|| {
		provide_context(state);
		provide_context(response.clone());
		provide_context(parts);
	});
	(owner, response)
}

fn response_cookie(response: &ResponseOptions) -> Cookie<'static> {
	let value = response.0.read().unwrap().headers[SET_COOKIE]
		.to_str()
		.unwrap()
		.to_string();
	Cookie::parse_encoded(value).unwrap().into_owned()
}

#[test]
fn expiring_user_tokens_require_complete_refresh_credentials() {
	let incomplete = GitHubUserTokenResponse {
		access_token: Some("access".to_string()),
		expires_in: Some(28_800),
		refresh_token: None,
		refresh_token_expires_in: None,
	};
	assert!(
		incomplete
			.into_token()
			.unwrap_err()
			.to_string()
			.contains("without refresh credentials")
	);

	let legacy = GitHubUserTokenResponse {
		access_token: Some("legacy-access".to_string()),
		expires_in: None,
		refresh_token: None,
		refresh_token_expires_in: None,
	}
	.into_token()
	.unwrap();
	assert_eq!(legacy.access_token, "legacy-access");
	assert!(legacy.access_token_expires_at.is_none());
}

#[tokio::test]
async fn login_sets_the_cookie_needed_to_validate_the_returned_state() {
	let state = state().await;
	let (owner, response) = context(state.clone(), None);
	let login_url = owner
		.with(|| ScopedFuture::new(get_login_url()))
		.await
		.unwrap();
	let login_url = url::Url::parse(&login_url).unwrap();
	let nonce = login_url
		.query_pairs()
		.find(|(name, _)| name == "state")
		.unwrap()
		.1
		.into_owned();
	assert!(
		login_url
			.query_pairs()
			.any(|(name, value)| name == "client_id" && value == "client-test-id")
	);
	assert!(login_url.query_pairs().any(|(name, value)| {
		name == "redirect_uri" && value == "https://monochange.dev/auth/callback"
	}));
	assert!(
		login_url
			.query_pairs()
			.any(|(name, value)| { name == "code_challenge_method" && value == "S256" })
	);
	assert!(
		login_url
			.query_pairs()
			.any(|(name, value)| name == "code_challenge" && !value.is_empty())
	);
	assert!(!login_url.query_pairs().any(|(name, _)| name == "scope"));
	let cookie = response_cookie(&response);
	assert_eq!(cookie.name(), oauth::OAUTH_COOKIE_NAME);
	assert_eq!(cookie.secure(), Some(true));
	let verified =
		oauth::verify_login_state(&state.jwt_secret, &CookieJar::new().add(cookie), &nonce)
			.unwrap();
	assert_eq!(verified.intent, oauth::OAuthIntent::Login);
	assert!(verified.code_verifier.is_some());
}

#[tokio::test]
async fn exchange_rejects_missing_or_mismatched_state_before_contacting_github() {
	let state = state().await;
	let pending = oauth::login_state(&state.jwt_secret).unwrap();
	for cookie in [None, Some(pending.cookie.to_string())] {
		let (owner, response) = context(state.clone(), cookie);
		let result = owner
			.with(|| {
				ScopedFuture::new(exchange_code(
					"must-not-be-used".to_string(),
					"wrong-state".to_string(),
				))
			})
			.await;
		assert_eq!(
			result.unwrap_err().to_string(),
			"error running server function: Invalid or expired OAuth state"
		);
		assert!(!response.0.read().unwrap().headers.contains_key(SET_COOKIE));
	}
}

#[tokio::test]
async fn valid_state_is_consumed_even_when_the_code_exchange_fails() {
	let state = state().await;
	let pending = oauth::login_state(&state.jwt_secret).unwrap();
	let (owner, response) = context(state, Some(pending.cookie.to_string()));
	let result = owner
		.with(|| {
			ScopedFuture::new(exchange_code(
				"invalid-test-code".to_string(),
				pending.state,
			))
		})
		.await;
	assert!(result.unwrap_err().to_string().contains("Token:"));
	let cleared = response_cookie(&response);
	assert_eq!(cleared.name(), oauth::OAUTH_COOKIE_NAME);
	assert_eq!(cleared.max_age(), Some(time::Duration::ZERO));
	assert_eq!(cleared.secure(), Some(true));
}

#[tokio::test]
async fn installation_authorization_exchanges_without_pkce_and_creates_a_session() {
	let server = MockServer::start();
	let exchange = server.mock(|when, then| {
		when.method(POST)
			.path("/login/oauth/access_token")
			.body_includes("code=installation-code")
			.body_includes("redirect_uri=")
			.body_excludes("code_verifier=");
		then.json_body(serde_json::json!({
			"access_token": "installation-access",
			"expires_in": 28800,
			"refresh_token": "installation-refresh",
			"refresh_token_expires_in": 15_897_600,
		}));
	});
	let user = server.mock(|when, then| {
		when.method(GET)
			.path("/user")
			.header("authorization", "Bearer installation-access");
		then.json_body(serde_json::json!({
			"id": 101,
			"login": "alice",
			"avatar_url": null,
		}));
	});
	let state = state_with_server(&server).await;
	let pending = oauth::installation_state(&state.jwt_secret).unwrap();
	let (owner, response) = context(state.clone(), Some(pending.cookie.to_string()));

	let session = owner
		.with(|| {
			ScopedFuture::new(exchange_code(
				"installation-code".to_string(),
				pending.state,
			))
		})
		.await
		.unwrap();

	assert_eq!(session.github_id, 101);
	assert_eq!(session.github_login, "alice");
	exchange.assert_calls(1);
	user.assert_calls(1);
	let cookies: Vec<String> = {
		let headers = response.0.read().unwrap();
		headers
			.headers
			.get_all(SET_COOKIE)
			.iter()
			.map(|value| value.to_str().unwrap().to_string())
			.collect()
	};
	assert!(cookies.iter().any(|cookie| {
		cookie.starts_with(oauth::OAUTH_COOKIE_NAME) && cookie.contains("Max-Age=0")
	}));
	assert!(
		cookies
			.iter()
			.any(|cookie| cookie.starts_with(oauth::SESSION_COOKIE_NAME))
	);
	let stored: (String, String, i64, i64) = sqlx::query_as(
		"SELECT github_access_token, github_refresh_token,
		        github_access_token_expires_at, github_refresh_token_expires_at
		 FROM users WHERE github_id = 101",
	)
	.fetch_one(&state.db)
	.await
	.unwrap();
	assert_eq!(stored.0, "installation-access");
	assert_eq!(stored.1, "installation-refresh");
	assert!(stored.2 > chrono::Utc::now().timestamp());
	assert!(stored.3 > stored.2);
}

#[tokio::test]
async fn logout_expires_the_host_only_session_cookie() {
	let (owner, response) = context(state().await, None);
	owner.with(|| ScopedFuture::new(logout())).await.unwrap();
	let cookie = response_cookie(&response);
	assert_eq!(cookie.name(), oauth::SESSION_COOKIE_NAME);
	assert_eq!(cookie.max_age(), Some(time::Duration::ZERO));
	assert_eq!(cookie.secure(), Some(true));
	assert_eq!(cookie.http_only(), Some(true));
	assert_eq!(cookie.path(), Some("/"));
	assert_eq!(cookie.domain(), None);
}

#[tokio::test]
async fn session_reads_the_host_only_cookie_and_ignores_the_old_name() {
	let state = state().await;
	let (owner, _) = context(
		state.clone(),
		Some("monochange_session=old-cookie".to_string()),
	);
	assert!(
		owner
			.with(|| ScopedFuture::new(get_session()))
			.await
			.unwrap()
			.is_none()
	);
	let (owner, _) = context(
		state,
		Some(format!("{}=invalid-token", oauth::SESSION_COOKIE_NAME)),
	);
	assert!(
		owner
			.with(|| ScopedFuture::new(get_session()))
			.await
			.unwrap_err()
			.to_string()
			.contains("Invalid session")
	);
}

#[tokio::test]
async fn expired_github_app_token_is_refreshed_once_for_concurrent_requests() {
	let server = MockServer::start();
	let refresh = server.mock(|when, then| {
		when.method(POST)
			.path("/login/oauth/access_token")
			.body_includes("grant_type=refresh_token")
			.body_includes("refresh_token=old-refresh");
		then.json_body(serde_json::json!({
			"access_token": "new-access",
			"expires_in": 28800,
			"refresh_token": "new-refresh",
			"refresh_token_expires_in": 15_897_600,
		}));
	});
	let state = state_with_server(&server).await;
	let now = chrono::Utc::now().timestamp();
	sqlx::query(
		"INSERT INTO users (
			id, github_id, github_login, github_access_token, github_refresh_token,
			github_access_token_expires_at, github_refresh_token_expires_at
		 ) VALUES (1, 101, 'alice', 'old-access', 'old-refresh', $1, $2)",
	)
	.bind(now - 1)
	.bind(now + 3600)
	.execute(&state.db)
	.await
	.unwrap();

	let (first, second) = tokio::join!(
		github_user_access_token(&state, 1),
		github_user_access_token(&state, 1),
	);
	assert_eq!(first.unwrap(), "new-access");
	assert_eq!(second.unwrap(), "new-access");
	refresh.assert_calls(1);
	let stored: (String, String, i64, i64) = sqlx::query_as(
		"SELECT github_access_token, github_refresh_token,
		        github_access_token_expires_at, github_refresh_token_expires_at
		 FROM users WHERE id = 1",
	)
	.fetch_one(&state.db)
	.await
	.unwrap();
	assert_eq!(stored.0, "new-access");
	assert_eq!(stored.1, "new-refresh");
	assert!(stored.2 > now);
	assert!(stored.3 > stored.2);
}

#[tokio::test]
async fn missing_user_cannot_receive_a_github_access_token() {
	let server = MockServer::start();
	let state = state_with_server(&server).await;
	let error = github_user_access_token(&state, 404)
		.await
		.unwrap_err()
		.to_string();
	assert!(error.contains("Invalid session"));
}

#[tokio::test]
async fn failed_token_refresh_requires_a_new_sign_in() {
	let server = MockServer::start();
	let state = state_with_server(&server).await;
	insert_expired_refreshable_user(&state).await;
	let mut state = (*state).clone();
	state.github_oauth_origin = "http://127.0.0.1:9".to_string();

	let error = github_user_access_token(&state, 1)
		.await
		.unwrap_err()
		.to_string();
	assert!(error.contains("please sign in again"));
}

#[tokio::test]
async fn malformed_token_refresh_response_requires_a_new_sign_in() {
	let server = MockServer::start();
	let refresh = server.mock(|when, then| {
		when.method(POST).path("/login/oauth/access_token");
		then.header("content-type", "application/json")
			.body("not-json");
	});
	let state = state_with_server(&server).await;
	insert_expired_refreshable_user(&state).await;

	let error = github_user_access_token(&state, 1)
		.await
		.unwrap_err()
		.to_string();
	assert!(error.contains("please sign in again"));
	refresh.assert_calls(1);
}

#[tokio::test]
async fn incomplete_token_refresh_response_requires_a_new_sign_in() {
	let server = MockServer::start();
	let refresh = server.mock(|when, then| {
		when.method(POST).path("/login/oauth/access_token");
		then.json_body(serde_json::json!({
			"access_token": "new-access",
			"expires_in": 28_800,
		}));
	});
	let state = state_with_server(&server).await;
	insert_expired_refreshable_user(&state).await;

	let error = github_user_access_token(&state, 1)
		.await
		.unwrap_err()
		.to_string();
	assert!(error.contains("please sign in again"));
	refresh.assert_calls(1);
}

#[tokio::test]
async fn expired_refresh_credentials_require_a_new_sign_in_without_contacting_github() {
	let server = MockServer::start();
	let outbound = server.mock(|when, then| {
		when.any_request();
		then.status(500);
	});
	let state = state_with_server(&server).await;
	let now = chrono::Utc::now().timestamp();
	sqlx::query(
		"INSERT INTO users (
			id, github_id, github_login, github_access_token, github_refresh_token,
			github_access_token_expires_at, github_refresh_token_expires_at
		 ) VALUES (1, 101, 'alice', 'old-access', 'old-refresh', $1, $2)",
	)
	.bind(now - 1)
	.bind(now - 1)
	.execute(&state.db)
	.await
	.unwrap();

	let error = github_user_access_token(&state, 1)
		.await
		.unwrap_err()
		.to_string();
	assert!(error.contains("please sign in again"));
	outbound.assert_calls(0);
}
