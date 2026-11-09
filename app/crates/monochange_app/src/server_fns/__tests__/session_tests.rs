//! Browser-only session management, the same-origin guard, and the device gate.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::Request;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header;
use axum::http::header::COOKIE;
use axum::http::header::ORIGIN;
use axum::http::header::SET_COOKIE;
use axum_extra::extract::cookie::CookieJar;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use leptos_axum::ResponseOptions;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::create_token;
use monochange_app_api::oauth;
use monochange_app_api::sessions::BrowserPrincipal;
use monochange_app_api::sessions::ClientPrincipal;
use monochange_app_api::sessions::Principal;
use monochange_app_db::client_sessions;
use monochange_shared::ClientKind;
use tower::ServiceExt;

use super::bearer_gate;
use super::connect_request;
use super::list_client_sessions;
use super::revoke_client_session;
use super::sign_out_everywhere;
use crate::server_fns::auth::get_session;

const NATIVE_REDIRECT: &str = "dev.monochange.app:/auth/native/return";
const SITE_ORIGIN: &str = "https://monochange.example";
const VERIFIER: &str = "known-verifier-known-verifier-known-verifier-known";

async fn state() -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	sqlx::query(
		"INSERT INTO users (id, github_id, github_login, github_access_token)
		 VALUES (1, 100, 'alice', 'test-only-token'), (2, 200, 'bob', 'test-only-token')",
	)
	.execute(&db)
	.await
	.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "session-management-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	let mut app_state = AppState::new(db, secrets).unwrap();
	app_state.github_callback_url = "https://monochange.example/auth/callback".into();
	Arc::new(app_state)
}

fn cookie(state: &AppState, user_id: i32, github_id: i64) -> String {
	let token = create_token(&state.jwt_secret, user_id, github_id, "alice", 0).unwrap();
	format!("{}={token}", oauth::SESSION_COOKIE_NAME)
}

type Context = (Owner, ResponseOptions);

fn context_with(state: &Arc<AppState>, build: impl FnOnce(&mut Request<()>)) -> Context {
	let owner = Owner::new();
	let response = ResponseOptions::default();
	let mut request = Request::new(());
	build(&mut request);
	let (parts, ()) = request.into_parts();
	owner.with(|| {
		provide_context(state.clone());
		provide_context(parts);
		provide_context(response.clone());
	});
	(owner, response)
}

fn context(state: &Arc<AppState>, cookie: Option<String>, origin: Option<&'static str>) -> Context {
	context_with(state, |request| {
		if let Some(cookie) = cookie {
			request
				.headers_mut()
				.insert(COOKIE, cookie.parse().unwrap());
		}
		if let Some(origin) = origin {
			request
				.headers_mut()
				.insert(ORIGIN, origin.parse().unwrap());
		}
	})
}

async fn client_session(state: &AppState, user_id: i32, token: &str) -> String {
	client_sessions::create_session(&state.db, user_id, "native", 0, token)
		.await
		.unwrap()
		.id
}

#[tokio::test]
async fn connect_request_previews_a_valid_handoff_for_the_browser_user() {
	let state = state().await;
	let (owner, _) = context(&state, Some(cookie(&state, 1, 100)), None);
	let preview = owner
		.with(|| {
			ScopedFuture::new(connect_request(
				"native".to_owned(),
				NATIVE_REDIRECT.to_owned(),
				monochange_shared::code_challenge(VERIFIER),
				"state-1234567890".to_owned(),
			))
		})
		.await
		.unwrap()
		.unwrap();
	assert_eq!(preview.client, ClientKind::Native);
	assert_eq!(preview.user.github_id, 100);
	assert_eq!(preview.user.github_login, "alice");
}

#[tokio::test]
async fn connect_request_returns_none_for_invalid_handoff_input() {
	let state = state().await;
	let (owner, _) = context(&state, Some(cookie(&state, 1, 100)), None);
	let run =
		|client: &'static str, redirect: &'static str, challenge: String, state_value: String| {
			let owner = owner.clone();
			async move {
				owner
					.with(|| {
						ScopedFuture::new(connect_request(
							client.to_owned(),
							redirect.to_owned(),
							challenge,
							state_value,
						))
					})
					.await
			}
		};

	assert!(
		run(
			"native",
			"https://attacker.example/cb",
			monochange_shared::code_challenge(VERIFIER),
			"state-1234567890".to_owned()
		)
		.await
		.unwrap()
		.is_none(),
		"an unregistered redirect must not preview"
	);
	assert!(
		run(
			"native",
			NATIVE_REDIRECT,
			"a".repeat(42),
			"state-1234567890".to_owned()
		)
		.await
		.unwrap()
		.is_none(),
		"a malformed challenge must not preview"
	);
	assert!(
		run(
			"native",
			NATIVE_REDIRECT,
			monochange_shared::code_challenge(VERIFIER),
			"short".to_owned()
		)
		.await
		.unwrap()
		.is_none(),
		"a malformed state must not preview"
	);
	assert!(
		run(
			"web",
			NATIVE_REDIRECT,
			monochange_shared::code_challenge(VERIFIER),
			"state-1234567890".to_owned()
		)
		.await
		.unwrap_err()
		.to_string()
		.contains("Unsupported client"),
	);
}

#[tokio::test]
async fn connect_request_requires_a_browser_principal() {
	let state = state().await;

	let (owner, _) = context(&state, None, None);
	assert!(
		owner
			.with(|| {
				ScopedFuture::new(connect_request(
					"native".to_owned(),
					NATIVE_REDIRECT.to_owned(),
					monochange_shared::code_challenge(VERIFIER),
					"state-1234567890".to_owned(),
				))
			})
			.await
			.unwrap_err()
			.to_string()
			.contains("Authentication required"),
	);

	// A client token is not a browser principal, even with a valid session.
	let token = client_session(&state, 1, "mcd_mobile").await;
	let (owner, _) = context_with(&state, |request| {
		request.headers_mut().insert(
			header::AUTHORIZATION,
			format!("Bearer {token}").parse().unwrap(),
		);
	});
	assert!(
		owner
			.with(|| {
				ScopedFuture::new(connect_request(
					"native".to_owned(),
					NATIVE_REDIRECT.to_owned(),
					monochange_shared::code_challenge(VERIFIER),
					"state-1234567890".to_owned(),
				))
			})
			.await
			.unwrap_err()
			.to_string()
			.contains("Authentication required"),
	);
}

#[tokio::test]
async fn list_and_revoke_manage_only_the_callers_own_sessions() {
	let state = state().await;
	let first = client_session(&state, 1, "mcd_first").await;
	let second = client_session(&state, 1, "mcd_second").await;
	client_session(&state, 2, "mcd_other").await;

	let (owner, _) = context(&state, Some(cookie(&state, 1, 100)), Some(SITE_ORIGIN));
	let listed = owner
		.with(|| ScopedFuture::new(list_client_sessions()))
		.await
		.unwrap();
	assert_eq!(
		listed
			.iter()
			.map(|session| session.id.as_str())
			.collect::<Vec<_>>(),
		vec![second.as_str(), first.as_str()]
	);
	assert_eq!(listed[0].client, ClientKind::Native);
	assert_eq!(listed[1].client, ClientKind::Native);

	owner
		.with(|| ScopedFuture::new(revoke_client_session(first.clone())))
		.await
		.unwrap();
	assert!(
		client_sessions::resolve_session(&state.db, "mcd_first")
			.await
			.unwrap()
			.is_none()
	);
	assert!(
		client_sessions::resolve_session(&state.db, "mcd_second")
			.await
			.unwrap()
			.is_some()
	);

	// Revoking an id the caller does not own is idempotent and leaks nothing.
	owner
		.with(|| ScopedFuture::new(revoke_client_session("mcs_not-yours".to_owned())))
		.await
		.unwrap();
	assert!(
		client_sessions::resolve_session(&state.db, "mcd_other")
			.await
			.unwrap()
			.is_some(),
		"another user's session must survive"
	);
}

#[tokio::test]
async fn state_changing_session_calls_require_the_site_origin() {
	let state = state().await;

	for origin in [None, Some("https://evil.example")] {
		let (owner, _) = context(&state, Some(cookie(&state, 1, 100)), origin);
		assert!(
			owner
				.with(|| ScopedFuture::new(revoke_client_session("mcs_any".to_owned())))
				.await
				.unwrap_err()
				.to_string()
				.contains("Cross-origin request rejected"),
			"revocation must reject origin {origin:?}"
		);
		assert!(
			owner
				.with(|| ScopedFuture::new(sign_out_everywhere()))
				.await
				.unwrap_err()
				.to_string()
				.contains("Cross-origin request rejected"),
			"sign-out must reject origin {origin:?}"
		);
	}

	// The site's own origin passes.
	let (owner, _) = context(&state, Some(cookie(&state, 1, 100)), Some(SITE_ORIGIN));
	owner
		.with(|| ScopedFuture::new(revoke_client_session("mcs_missing".to_owned())))
		.await
		.unwrap();
}

#[tokio::test]
async fn sign_out_everywhere_revokes_every_credential_and_clears_the_cookie() {
	let state = state().await;
	let browser = cookie(&state, 1, 100);
	client_session(&state, 1, "mcd_doomed").await;
	client_sessions::issue_grant(
		&state.db,
		1,
		"native",
		NATIVE_REDIRECT,
		&monochange_shared::code_challenge(VERIFIER),
		0,
	)
	.await
	.unwrap();

	let (owner, response) = context(&state, Some(browser.clone()), Some(SITE_ORIGIN));
	owner
		.with(|| ScopedFuture::new(sign_out_everywhere()))
		.await
		.unwrap();

	let epoch: i64 = sqlx::query_scalar("SELECT session_epoch FROM users WHERE id = 1")
		.fetch_one(&state.db)
		.await
		.unwrap();
	assert_eq!(epoch, 1);
	assert!(
		client_sessions::resolve_session(&state.db, "mcd_doomed")
			.await
			.unwrap()
			.is_none()
	);
	let grants: i64 = sqlx::query_scalar("SELECT count(*) FROM client_grants")
		.fetch_one(&state.db)
		.await
		.unwrap();
	assert_eq!(grants, 0);

	// The old browser JWT no longer resolves through the one resolver.
	let mut headers = HeaderMap::new();
	headers.insert(COOKIE, browser.parse().unwrap());
	let old_principal =
		monochange_app_api::sessions::resolve_parts(&state, &headers, &CookieJar::new())
			.await
			.unwrap();
	assert_eq!(
		old_principal.user_id(),
		None,
		"the old cookie must not authorize"
	);
	let cookies: Vec<String> = response
		.0
		.read()
		.unwrap()
		.headers
		.get_all(SET_COOKIE)
		.iter()
		.map(|value| value.to_str().unwrap().to_owned())
		.collect();
	assert!(
		cookies.iter().any(|cookie| {
			cookie.starts_with(oauth::SESSION_COOKIE_NAME) && cookie.contains("Max-Age=0")
		}),
		"the browser cookie must be cleared: {cookies:?}"
	);
}

#[tokio::test]
async fn the_middleware_principal_takes_precedence_over_request_credentials() {
	let state = state().await;
	let principal = Principal::Browser(BrowserPrincipal {
		user_id: 2,
		github_id: 200,
		github_login: "bob".to_owned(),
		github_avatar_url: None,
		plan_tier: "free".to_owned(),
		epoch: 0,
	});
	let (owner, _) = context_with(&state, |request| {
		// Alice's cookie is present, but the middleware already resolved Bob.
		request
			.headers_mut()
			.insert(COOKIE, cookie(&state, 1, 100).parse().unwrap());
		request.extensions_mut().insert(principal);
	});

	let user = owner
		.with(|| ScopedFuture::new(get_session()))
		.await
		.unwrap()
		.unwrap();
	assert_eq!(user.github_login, "bob", "the resolved extension wins");
}

fn gate_router() -> Router {
	use axum::routing::get;
	use axum::routing::post;

	Router::new()
		.route("/api/get_session", post(|| async { "session" }))
		.route(
			"/api/sign_out_everywhere",
			post(|| async { "browser-only" }),
		)
		.route("/api/list_repos", post(|| async { "read-only" }))
		.route("/api/client-sessions/token", post(|| async { "handoff" }))
		.route(
			"/api/client-sessions/revoke",
			post(|| async { StatusCode::NO_CONTENT }),
		)
		.route("/health", get(|| async { "public" }))
		.layer(axum::middleware::from_fn(bearer_gate))
}

fn client_principal() -> Principal {
	Principal::Client(ClientPrincipal {
		session_id: "mcs_gate".to_owned(),
		user_id: 1,
		github_id: 100,
		github_login: "alice".to_owned(),
		github_avatar_url: None,
		plan_tier: "free".to_owned(),
		client: ClientKind::Native,
		epoch: 0,
	})
}

async fn send_to_gate(
	path: &'static str,
	principal: Option<Principal>,
) -> axum::response::Response {
	let mut request = Request::builder()
		.method("POST")
		.uri(path)
		.body(Body::empty())
		.unwrap();
	if let Some(principal) = principal {
		request.extensions_mut().insert(principal);
	}
	gate_router().oneshot(request).await.unwrap()
}

async fn send_method_to_gate(
	method: &'static str,
	path: &'static str,
	principal: Option<Principal>,
) -> axum::response::Response {
	let mut request = Request::builder()
		.method(method)
		.uri(path)
		.body(Body::empty())
		.unwrap();
	if let Some(principal) = principal {
		request.extensions_mut().insert(principal);
	}
	gate_router().oneshot(request).await.unwrap()
}

#[tokio::test]
async fn client_tokens_cannot_reach_browser_only_endpoints() {
	let response = send_to_gate("/api/sign_out_everywhere", Some(client_principal())).await;
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
	let body = axum::body::to_bytes(response.into_body(), 4096)
		.await
		.unwrap();
	assert!(
		!body.is_empty(),
		"the server fn error encoding must be sent"
	);

	// Project CRUD server functions stay browser-only for this unit.
	let response = send_to_gate("/api/list_repos", Some(client_principal())).await;
	assert_eq!(response.status(), StatusCode::FORBIDDEN);

	let response = send_to_gate("/api/client-sessions/token", Some(client_principal())).await;
	assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn client_tokens_keep_the_read_only_surface_and_self_revocation() {
	let response = send_to_gate("/api/get_session", Some(client_principal())).await;
	assert_eq!(response.status(), StatusCode::OK);
	let response = send_to_gate("/api/client-sessions/revoke", Some(client_principal())).await;
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	// Non-server-fn paths never gate, and an invalid principal passes through
	// so the endpoint itself reports the credential problem.
	let response = send_method_to_gate("GET", "/health", Some(client_principal())).await;
	assert_eq!(response.status(), StatusCode::OK);
	let response = send_to_gate("/api/get_session", Some(Principal::Invalid)).await;
	assert_eq!(response.status(), StatusCode::OK);
}

/// The SSR chain at `GET /dashboard`: the real principal middleware and
/// bearer gate resolve the request, then the page's server functions run
/// against those request parts exactly as `leptos_axum` provides them.
async fn dashboard_session(request: axum::extract::Request) -> String {
	let owner = Owner::new();
	let response_options = ResponseOptions::default();
	let (parts, _body) = request.into_parts();
	owner.with(|| {
		provide_context(parts);
		provide_context(response_options);
	});
	let session = owner.with(|| ScopedFuture::new(get_session())).await;
	match session {
		Ok(Some(user)) => format!("user:{}", user.github_login),
		Ok(None) => "anonymous".to_owned(),
		Err(error) => format!("error:{error}"),
	}
}

fn chain_router(state: Arc<AppState>) -> Router {
	use axum::middleware::from_fn;
	use axum::middleware::from_fn_with_state;
	use axum::routing::get;
	use axum::routing::post;

	Router::new()
		.route("/dashboard", get(dashboard_session))
		.route("/api/get_session", post(|| async { "stub-session" }))
		.route(
			"/api/sign_out_everywhere",
			post(|| async { "stub-browser-only" }),
		)
		// The same layer order main.rs mounts: the resolver runs first and
		// the gate sees the principal it inserted.
		.layer(from_fn(bearer_gate))
		.layer(from_fn_with_state(
			state,
			monochange_app_api::sessions::principal_middleware,
		))
}

#[tokio::test]
async fn the_ssr_chain_resolves_the_signed_in_browser_user() {
	let state = state().await;
	let router = chain_router(state.clone());

	let request = Request::builder()
		.method("GET")
		.uri("/dashboard")
		.header(COOKIE, cookie(&state, 1, 100))
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	let body = axum::body::to_bytes(response.into_body(), 128)
		.await
		.unwrap();
	assert_eq!(&body[..], b"user:alice");

	// No credentials render anonymous, not an error.
	let request = Request::builder()
		.method("GET")
		.uri("/dashboard")
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	let body = axum::body::to_bytes(response.into_body(), 128)
		.await
		.unwrap();
	assert_eq!(&body[..], b"anonymous");
}

#[tokio::test]
async fn the_ssr_chain_reports_forged_credentials_and_never_falls_back() {
	let state = state().await;
	let router = chain_router(state.clone());

	// A forged cookie is the pinned browser signed-out UX: an error, never
	// an anonymous dashboard.
	let forged = format!("{}=forged", oauth::SESSION_COOKIE_NAME);
	let request = Request::builder()
		.method("GET")
		.uri("/dashboard")
		.header(COOKIE, forged)
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	let body = axum::body::to_bytes(response.into_body(), 256)
		.await
		.unwrap();
	let text = String::from_utf8(body.to_vec()).unwrap();
	assert!(
		text.starts_with("error:") && text.contains("Invalid session"),
		"got {text}"
	);

	// A malformed bearer never falls back to the valid cookie behind it.
	let request = Request::builder()
		.method("GET")
		.uri("/dashboard")
		.header(COOKIE, cookie(&state, 1, 100))
		.header(header::AUTHORIZATION, "Bearer mcd_wrong")
		.body(Body::empty())
		.unwrap();
	let response = router.oneshot(request).await.unwrap();
	let body = axum::body::to_bytes(response.into_body(), 256)
		.await
		.unwrap();
	let text = String::from_utf8(body.to_vec()).unwrap();
	assert!(
		text.starts_with("error:") && text.contains("Invalid session"),
		"got {text}"
	);
}

#[tokio::test]
async fn rejected_bearers_get_401_and_browser_only_paths_stay_403_on_real_routes() {
	let state = state().await;
	let router = chain_router(state.clone());

	// An expired client credential answers 401 on the read-only surface
	// instead of a generic server-fn error body.
	client_session(&state, 1, "mcd_expired").await;
	client_sessions::revoke_session(&state.db, "mcd_expired")
		.await
		.unwrap();
	let request = Request::builder()
		.method("POST")
		.uri("/api/get_session")
		.header(header::AUTHORIZATION, "Bearer mcd_expired")
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

	// A malformed bearer never falls back to the valid cookie behind it.
	let request = Request::builder()
		.method("POST")
		.uri("/api/get_session")
		.header(COOKIE, cookie(&state, 1, 100))
		.header(header::AUTHORIZATION, "Basic nonsense")
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

	// A valid device credential stays off browser-only endpoints: 403.
	client_session(&state, 1, "mcd_live").await;
	let request = Request::builder()
		.method("POST")
		.uri("/api/sign_out_everywhere")
		.header(header::AUTHORIZATION, "Bearer mcd_live")
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	assert_eq!(response.status(), StatusCode::FORBIDDEN);

	// A rejected bearer on an SSR path keeps the pinned browser UX: the
	// gate only rewrites server-function requests.
	let request = Request::builder()
		.method("GET")
		.uri("/dashboard")
		.header(COOKIE, cookie(&state, 1, 100))
		.header(header::AUTHORIZATION, "Bearer mcd_wrong")
		.body(Body::empty())
		.unwrap();
	let response = router.clone().oneshot(request).await.unwrap();
	let body = axum::body::to_bytes(response.into_body(), 256)
		.await
		.unwrap();
	let text = String::from_utf8(body.to_vec()).unwrap();
	assert!(
		text.starts_with("error:") && text.contains("Invalid session"),
		"got {text}"
	);

	// A forged cookie (no Authorization header) still reaches the server
	// functions on the API surface.
	let request = Request::builder()
		.method("POST")
		.uri("/api/get_session")
		.header(COOKIE, format!("{}=forged", oauth::SESSION_COOKIE_NAME))
		.body(Body::empty())
		.unwrap();
	let response = router.oneshot(request).await.unwrap();
	assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn a_resolution_outage_on_a_server_fn_path_answers_503_in_leptos_encoding() {
	use server_fn::error::FromServerFnError;

	let state = state().await;
	let router = chain_router(state.clone());

	state.db.close().await;

	let request = Request::builder()
		.method("POST")
		.uri("/api/get_session")
		.header(COOKIE, cookie(&state, 1, 100))
		.body(Body::empty())
		.unwrap();
	let response = router.oneshot(request).await.unwrap();
	assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

	// The body uses the Leptos server-function error encoding, so the wasm
	// client decodes the outage instead of reporting a transport failure.
	let parts = server_fn::ServerFnError::new("encoding probe").ser();
	assert_eq!(
		response.headers().get(header::CONTENT_TYPE).unwrap(),
		parts.content_type
	);
	let body = axum::body::to_bytes(response.into_body(), 4096)
		.await
		.unwrap();
	assert!(!body.is_empty());
}

#[tokio::test]
async fn browser_and_anonymous_principals_pass_the_gate() {
	let browser = Principal::Browser(BrowserPrincipal {
		user_id: 1,
		github_id: 100,
		github_login: "alice".to_owned(),
		github_avatar_url: None,
		plan_tier: "free".to_owned(),
		epoch: 0,
	});
	let response = send_to_gate("/api/sign_out_everywhere", Some(browser)).await;
	assert_eq!(response.status(), StatusCode::OK);
	let response = send_to_gate("/api/sign_out_everywhere", Some(Principal::Anonymous)).await;
	assert_eq!(response.status(), StatusCode::OK);

	// A server fn path without any resolved principal is a wiring error.
	let response = send_to_gate("/api/sign_out_everywhere", None).await;
	assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
	let response = send_method_to_gate("GET", "/health", None).await;
	assert_eq!(response.status(), StatusCode::OK);
}
