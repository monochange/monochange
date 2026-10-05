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
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use leptos_axum::ResponseOptions;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::oauth;

use super::exchange_code;
use super::get_login_url;
use super::get_session;
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
	let cookie = response_cookie(&response);
	assert_eq!(cookie.name(), oauth::OAUTH_COOKIE_NAME);
	assert_eq!(cookie.secure(), Some(true));
	assert!(
		oauth::verify_login_state(&state.jwt_secret, &CookieJar::new().add(cookie), &nonce).is_ok()
	);
}

#[tokio::test]
async fn exchange_rejects_missing_or_mismatched_state_before_contacting_github() {
	let state = state().await;
	let (_, cookie) = oauth::login_state(&state.jwt_secret).unwrap();
	for cookie in [None, Some(cookie.to_string())] {
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
	let (nonce, cookie) = oauth::login_state(&state.jwt_secret).unwrap();
	let (owner, response) = context(state, Some(cookie.to_string()));
	let result = owner
		.with(|| ScopedFuture::new(exchange_code("invalid-test-code".to_string(), nonce)))
		.await;
	assert!(result.unwrap_err().to_string().contains("Token:"));
	let cleared = response_cookie(&response);
	assert_eq!(cleared.name(), oauth::OAUTH_COOKIE_NAME);
	assert_eq!(cleared.max_age(), Some(time::Duration::ZERO));
	assert_eq!(cleared.secure(), Some(true));
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
