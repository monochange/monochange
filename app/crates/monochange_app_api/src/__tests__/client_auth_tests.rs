//! The handoff endpoints: code burning, origin rules, and redirect safety.

#![allow(clippy::disallowed_methods)]

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::Response;
use axum_extra::extract::cookie::CookieJar;
use monochange_app_db::client_sessions;
use monochange_shared::ClientKind;
use monochange_shared::ClientSession;
use monochange_shared::ClientTokenRequest;
use monochange_shared::code_challenge;
use tower::ServiceExt;

use crate::AppState;
use crate::client_auth::APPROVE_PATH;
use crate::client_auth::TOKEN_PATH;
use crate::client_auth::redirect_allowed;
use crate::oauth::SESSION_COOKIE_NAME;

const SITE_ORIGIN: &str = "https://monochange.example";
const NATIVE_REDIRECT: &str = "dev.monochange.app:/auth/native/return";
const VERIFIER: &str = "known-verifier-known-verifier-known-verifier-known";

async fn state() -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: crate::AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "handoff-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	let mut app_state = AppState::new(db, secrets).unwrap();
	app_state.github_callback_url = "https://monochange.example/auth/callback".into();
	app_state
}

async fn insert_user(state: &AppState, id: i32) {
	sqlx::query(
		"INSERT INTO users (id, github_id, github_login, github_access_token)
		 VALUES (?, ?, ?, '')",
	)
	.bind(id)
	.bind(i64::from(id) * 100)
	.bind(format!("user-{id}"))
	.execute(&state.db)
	.await
	.unwrap();
}

async fn issue_native_grant(state: &AppState, verifier: &str) -> String {
	client_sessions::issue_grant(
		&state.db,
		1,
		"native",
		NATIVE_REDIRECT,
		&code_challenge(verifier),
		0,
	)
	.await
	.unwrap()
}

fn token_request(client: ClientKind, code: &str, verifier: &str, redirect_uri: &str) -> String {
	serde_json::to_string(&ClientTokenRequest {
		client,
		code: code.to_owned(),
		code_verifier: verifier.to_owned(),
		redirect_uri: redirect_uri.to_owned(),
	})
	.unwrap()
}

async fn post_json(
	state: &AppState,
	path: &str,
	body: String,
	origin: Option<&'static str>,
) -> Response {
	let mut request = Request::builder()
		.method("POST")
		.uri(path)
		.header(header::CONTENT_TYPE, "application/json")
		.body(Body::from(body))
		.unwrap();
	if let Some(origin) = origin {
		request
			.headers_mut()
			.insert(header::ORIGIN, HeaderValue::from_static(origin));
	}
	router(state).oneshot(request).await.unwrap()
}

fn approve_form(client: &str, redirect_uri: &str, challenge: &str, state: &str) -> String {
	url::form_urlencoded::Serializer::new(String::new())
		.append_pair("client", client)
		.append_pair("redirect_uri", redirect_uri)
		.append_pair("code_challenge", challenge)
		.append_pair("state", state)
		.finish()
}

async fn post_approve(
	state: &AppState,
	cookie: Option<String>,
	origin: Option<&'static str>,
	bearer: Option<&str>,
	body: String,
) -> Response {
	let mut builder = Request::builder()
		.method("POST")
		.uri(APPROVE_PATH)
		.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
	if let Some(cookie) = cookie {
		builder = builder.header(header::COOKIE, cookie);
	}
	if let Some(origin) = origin {
		builder = builder.header(header::ORIGIN, HeaderValue::from_static(origin));
	}
	if let Some(bearer) = bearer {
		builder = builder.header(header::AUTHORIZATION, format!("Bearer {bearer}"));
	}
	router(state)
		.oneshot(builder.body(Body::from(body)).unwrap())
		.await
		.unwrap()
}

fn router(state: &AppState) -> Router {
	crate::api_router(state.clone())
}

fn session_cookie(state: &AppState, user_id: i32) -> String {
	let token = crate::create_token(
		&state.jwt_secret,
		user_id,
		i64::from(user_id) * 100,
		"alice",
		0,
	)
	.unwrap();
	format!("{SESSION_COOKIE_NAME}={token}")
}

async fn grant_count(state: &AppState) -> i64 {
	sqlx::query_scalar("SELECT count(*) FROM client_grants")
		.fetch_one(&state.db)
		.await
		.unwrap()
}

#[tokio::test]
async fn a_valid_handoff_exchanges_the_code_for_a_client_session() {
	let state = state().await;
	insert_user(&state, 1).await;
	let code = issue_native_grant(&state, VERIFIER).await;

	let response = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(response.status(), StatusCode::OK);
	assert_eq!(
		response.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);

	let bytes = axum::body::to_bytes(response.into_body(), 4096)
		.await
		.unwrap();
	let session: ClientSession = serde_json::from_slice(&bytes).unwrap();
	assert!(session.id.starts_with("mcs_"));
	assert!(session.access_token.starts_with("mcd_"));
	assert!(
		session.expires_at > chrono::Utc::now().timestamp() + 29 * 24 * 60 * 60,
		"the token should live about 30 days"
	);
	assert_eq!(session.user.github_id, 100);
	assert_eq!(session.user.github_login, "user-1");

	// The token resolves to the same database identity through the one
	// resolver, with the same epoch the grant was bound to.
	let mut headers = axum::http::HeaderMap::new();
	headers.insert(
		header::AUTHORIZATION,
		format!("Bearer {}", session.access_token).parse().unwrap(),
	);
	let principal = crate::sessions::resolve_parts(&state, &headers, &CookieJar::default())
		.await
		.unwrap();
	let crate::sessions::Principal::Client(resolved) = principal else {
		panic!("expected a client principal");
	};
	assert_eq!(resolved.user_id, 1);
	assert_eq!(resolved.client, ClientKind::Native);
	assert_eq!(resolved.session_id, session.id);
}

#[tokio::test]
async fn a_wrong_verifier_burns_the_code_even_for_a_later_correct_attempt() {
	let state = state().await;
	insert_user(&state, 1).await;
	let code = issue_native_grant(&state, VERIFIER).await;

	let wrong = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, &"b".repeat(43), NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(wrong.status(), StatusCode::BAD_REQUEST);
	assert_eq!(
		wrong.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);

	// The correct verifier cannot resurrect the burned code.
	let correct = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(correct.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn wrong_client_or_redirect_binding_burns_the_code() {
	let state = state().await;
	insert_user(&state, 1).await;
	let code = issue_native_grant(&state, VERIFIER).await;

	// The extension client is not registered for the native redirect.
	let other_client = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Extension, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(other_client.status(), StatusCode::BAD_REQUEST);

	// A different exact redirect is rejected.
	let other_redirect = post_json(
		&state,
		TOKEN_PATH,
		serde_json::to_string(&ClientTokenRequest {
			client: ClientKind::Native,
			code: code.clone(),
			code_verifier: VERIFIER.to_owned(),
			redirect_uri: "dev.monochange.app:/auth/other/return".to_owned(),
		})
		.unwrap(),
		None,
	)
	.await;
	assert_eq!(other_redirect.status(), StatusCode::BAD_REQUEST);

	let correct = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(correct.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn expired_and_replayed_grants_are_rejected() {
	let state = state().await;
	insert_user(&state, 1).await;

	let expired = issue_native_grant(&state, VERIFIER).await;
	sqlx::query("UPDATE client_grants SET expires_at = ?")
		.bind(chrono::Utc::now().timestamp() - 1)
		.execute(&state.db)
		.await
		.unwrap();
	let response = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &expired, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);

	let code = issue_native_grant(&state, VERIFIER).await;
	let first = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(first.status(), StatusCode::OK);
	let replay = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(replay.status(), StatusCode::BAD_REQUEST);
	assert_eq!(
		replay.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);
}

#[tokio::test]
async fn a_grant_issued_before_a_global_signout_fails() {
	let state = state().await;
	insert_user(&state, 1).await;
	let code = issue_native_grant(&state, VERIFIER).await;

	client_sessions::revoke_all_for_user(&state.db, 1)
		.await
		.unwrap();

	let response = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn approval_requires_a_browser_cookie_and_rejects_a_client_bearer() {
	let state = state().await;
	insert_user(&state, 1).await;
	let challenge = code_challenge(VERIFIER);
	let form = approve_form("native", NATIVE_REDIRECT, &challenge, "state-1234567890");

	let anonymous = post_approve(&state, None, Some(SITE_ORIGIN), None, form.clone()).await;
	assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);

	// A client token is not a browser principal and cannot approve itself.
	let bearer_only = post_approve(
		&state,
		None,
		Some(SITE_ORIGIN),
		Some("mcd_not-a-browser"),
		form.clone(),
	)
	.await;
	assert_eq!(bearer_only.status(), StatusCode::UNAUTHORIZED);

	// Cookie and bearer together still resolve to the client, which must not
	// approve: the browser cookie never rescues a bearer-only flow.
	let both = post_approve(
		&state,
		Some(session_cookie(&state, 1)),
		Some(SITE_ORIGIN),
		Some("mcd_still-not-a-browser"),
		form,
	)
	.await;
	assert_eq!(both.status(), StatusCode::UNAUTHORIZED);

	assert_eq!(grant_count(&state).await, 0);
}

#[tokio::test]
async fn approval_rejects_missing_and_foreign_origins() {
	let state = state().await;
	insert_user(&state, 1).await;
	let cookie = session_cookie(&state, 1);
	let form = approve_form(
		"native",
		NATIVE_REDIRECT,
		&code_challenge(VERIFIER),
		"state-1234567890",
	);

	let missing = post_approve(&state, Some(cookie.clone()), None, None, form.clone()).await;
	assert_eq!(missing.status(), StatusCode::FORBIDDEN);

	let foreign = post_approve(
		&state,
		Some(cookie),
		Some("https://evil.example"),
		None,
		form,
	)
	.await;
	assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
	assert_eq!(grant_count(&state).await, 0);
}

#[tokio::test]
async fn approval_redirects_with_only_an_opaque_code_and_state() {
	let state = state().await;
	insert_user(&state, 1).await;
	let cookie = session_cookie(&state, 1);
	let form = approve_form(
		"native",
		NATIVE_REDIRECT,
		&code_challenge(VERIFIER),
		"state-1234567890",
	);

	let response = post_approve(&state, Some(cookie), Some(SITE_ORIGIN), None, form).await;
	assert_eq!(response.status(), StatusCode::SEE_OTHER);
	let location = response
		.headers()
		.get(header::LOCATION)
		.unwrap()
		.to_str()
		.unwrap()
		.to_owned();
	let url = url::Url::parse(&location).unwrap();
	assert_eq!(url.scheme(), "dev.monochange.app");
	assert_eq!(url.path(), "/auth/native/return");
	let pairs: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
	let code = pairs.get("code").map(String::as_str).unwrap();
	assert!(code.starts_with("mcc_"));
	assert_eq!(
		pairs.get("state").map(String::as_str),
		Some("state-1234567890")
	);
	assert_eq!(
		pairs.len(),
		2,
		"no tokens or extra data may ride the redirect"
	);
	assert!(!location.contains("mcd_"));

	// The issued grant exchanges for a real session.
	let exchange = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(exchange.status(), StatusCode::OK);
}

#[tokio::test]
async fn approval_rejects_unregistered_redirects_and_malformed_input() {
	let state = state().await;
	insert_user(&state, 1).await;
	let cookie = session_cookie(&state, 1);
	let challenge = code_challenge(VERIFIER);

	for form in [
		approve_form(
			"native",
			"https://attacker.example/cb",
			&challenge,
			"state-1234567890",
		),
		approve_form(
			"native",
			NATIVE_REDIRECT,
			&"a".repeat(42),
			"state-1234567890",
		),
		approve_form("native", NATIVE_REDIRECT, &challenge, "short"),
		approve_form("web", NATIVE_REDIRECT, &challenge, "state-1234567890"),
	] {
		let response =
			post_approve(&state, Some(cookie.clone()), Some(SITE_ORIGIN), None, form).await;
		assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	}

	assert_eq!(grant_count(&state).await, 0);
}

#[tokio::test]
async fn the_token_endpoint_accepts_only_configured_extension_origins() {
	let mut state = state().await;
	insert_user(&state, 1).await;
	state.extension_redirect_uris = vec!["https://abc.chromiumapp.org/".to_owned()];

	let code = client_sessions::issue_grant(
		&state.db,
		1,
		"extension",
		"https://abc.chromiumapp.org/",
		&code_challenge(VERIFIER),
		0,
	)
	.await
	.unwrap();
	let request = token_request(
		ClientKind::Extension,
		&code,
		VERIFIER,
		"https://abc.chromiumapp.org/",
	);

	// The site's own browser origin cannot call the handoff, and the
	// shape-level rejection leaves the code usable by the real client.
	let web = post_json(&state, TOKEN_PATH, request.clone(), Some(SITE_ORIGIN)).await;
	assert_eq!(web.status(), StatusCode::BAD_REQUEST);

	let extension = post_json(
		&state,
		TOKEN_PATH,
		request,
		Some("https://abc.chromiumapp.org"),
	)
	.await;
	assert_eq!(extension.status(), StatusCode::OK);
	assert_eq!(
		extension.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);
}

#[tokio::test]
async fn oversized_handoff_bodies_are_rejected_and_leave_nothing_behind() {
	let state = state().await;
	let padding = "x".repeat(5000);
	let body = format!(
		r#"{{"client":"native","code":"{padding}","code_verifier":"{}","redirect_uri":"{NATIVE_REDIRECT}"}}"#,
		"a".repeat(43),
	);
	let response = post_json(&state, TOKEN_PATH, body, None).await;
	assert!(response.status().is_client_error());
	assert_eq!(
		response.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);
	assert_eq!(grant_count(&state).await, 0);

	// A code longer than the store accepts is rejected before any lookup.
	let long_code = "c".repeat(257);
	let body = format!(
		r#"{{"client":"native","code":"{long_code}","code_verifier":"{}","redirect_uri":"{NATIVE_REDIRECT}"}}"#,
		"a".repeat(43),
	);
	let response = post_json(&state, TOKEN_PATH, body, None).await;
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	assert_eq!(
		response.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);
}

#[tokio::test]
async fn an_epoch_drift_after_the_grant_was_issued_fails() {
	let state = state().await;
	insert_user(&state, 1).await;
	let code = issue_native_grant(&state, VERIFIER).await;

	// The user's epoch moved without deleting the grants (as a migration or
	// a partial write could); the mismatch must reject the exchange.
	sqlx::query("UPDATE users SET session_epoch = 1 WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();

	let response = post_json(
		&state,
		TOKEN_PATH,
		token_request(ClientKind::Native, &code, VERIFIER, NATIVE_REDIRECT),
		None,
	)
	.await;
	assert_eq!(response.status(), StatusCode::BAD_REQUEST);
	assert_eq!(
		response.headers().get(header::CACHE_CONTROL).unwrap(),
		"no-store"
	);
}

#[tokio::test]
async fn revocation_is_self_serving_idempotent_and_does_not_leak_existence() {
	let state = state().await;
	insert_user(&state, 1).await;
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_secret")
		.await
		.unwrap();

	let revoke = |token: &'static str| {
		let state = state.clone();
		async move {
			let request = Request::builder()
				.method("POST")
				.uri(crate::client_auth::REVOKE_PATH)
				.header(header::AUTHORIZATION, format!("Bearer {token}"))
				.body(Body::empty())
				.unwrap();
			router(&state).oneshot(request).await.unwrap()
		}
	};

	assert_eq!(revoke("mcd_secret").await.status(), StatusCode::NO_CONTENT);
	assert_eq!(revoke("mcd_secret").await.status(), StatusCode::NO_CONTENT);
	assert_eq!(
		revoke("mcd_never_existed").await.status(),
		StatusCode::NO_CONTENT
	);

	let mut headers = axum::http::HeaderMap::new();
	headers.insert(
		header::AUTHORIZATION,
		HeaderValue::from_static("Bearer mcd_secret"),
	);
	assert!(
		crate::sessions::resolve_parts(&state, &headers, &CookieJar::default())
			.await
			.unwrap()
			.user_id()
			.is_none()
	);
}

#[tokio::test]
async fn redirect_validation_rejects_unsafe_native_and_extension_addresses() {
	let mut state = state().await;

	// Native: the exact reverse-DNS return path is registered and safe.
	assert!(redirect_allowed(
		&state,
		ClientKind::Native,
		NATIVE_REDIRECT
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Native,
		"dev.monochange.app:/auth/other/return"
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Native,
		"https://attacker.example/cb"
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Native,
		"dev.monochange.app:/auth/native/return?stolen=1"
	));

	// Extension: nothing is registered until configured, and then only exact
	// https addresses without query, fragment, or credentials.
	assert!(!redirect_allowed(
		&state,
		ClientKind::Extension,
		"https://abc.chromiumapp.org/"
	));
	state.extension_redirect_uris = vec!["https://abc.chromiumapp.org/".to_owned()];
	assert!(redirect_allowed(
		&state,
		ClientKind::Extension,
		"https://abc.chromiumapp.org/"
	));

	// A configured address that fails the safety shape is still rejected, so
	// careless configuration cannot weaken the handoff.
	state.extension_redirect_uris = vec!["https://abc.chromiumapp.org/?x=1".to_owned()];
	assert!(!redirect_allowed(
		&state,
		ClientKind::Extension,
		"https://abc.chromiumapp.org/?x=1"
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Extension,
		"https://abc.chromiumapp.org/?x=1"
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Extension,
		"https://abc.chromiumapp.org/page#f"
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Extension,
		"http://abc.chromiumapp.org/"
	));
	assert!(!redirect_allowed(
		&state,
		ClientKind::Extension,
		"https://user:pw@abc.chromiumapp.org/"
	));
}
