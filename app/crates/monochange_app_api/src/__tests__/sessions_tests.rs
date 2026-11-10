//! The shared resolver's browser, client, and fallback behavior.

#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::http::HeaderMap;
use axum::http::header;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::cookie::CookieJar;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header as JwtHeader;
use jsonwebtoken::encode;
use monochange_app_db::client_sessions;
use monochange_shared::ClientKind;
use tower::ServiceExt;

use crate::AppSecrets;
use crate::AppState;
use crate::oauth::SESSION_COOKIE_NAME;
use crate::sessions::Principal;
use crate::sessions::resolve_parts;

async fn state() -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "session-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	AppState::new(db, secrets).unwrap()
}

async fn insert_user(state: &AppState, id: i32, github_id: i64) {
	sqlx::query(
		"INSERT INTO users (id, github_id, github_login, github_access_token, session_epoch)
		 VALUES (?, ?, ?, '', 0)",
	)
	.bind(id)
	.bind(github_id)
	.bind(format!("user-{id}"))
	.execute(&state.db)
	.await
	.unwrap();
}

fn browser_token(state: &AppState, epoch: i64) -> String {
	crate::create_token(&state.jwt_secret, 1, 101, "user-1", epoch).unwrap()
}

fn cookie_jar(token: &str) -> CookieJar {
	CookieJar::new().add(Cookie::new(SESSION_COOKIE_NAME, token.to_string()))
}

fn bearer_headers(token: &str) -> HeaderMap {
	let mut headers = HeaderMap::new();
	headers.insert(
		header::AUTHORIZATION,
		format!("Bearer {token}").parse().unwrap(),
	);
	headers
}

#[tokio::test]
async fn a_valid_browser_cookie_resolves_to_the_user() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	let token = browser_token(&state, 0);

	let principal = resolve_parts(&state, &HeaderMap::new(), &cookie_jar(&token))
		.await
		.unwrap();
	let Principal::Browser(user) = principal else {
		panic!("expected a browser principal");
	};
	assert_eq!(user.user_id, 1);
	assert_eq!(user.github_id, 101);
	assert_eq!(user.github_login, "user-1");
	assert_eq!(user.epoch, 0);
	assert_eq!(user.session_user().github_login, "user-1");
}

#[tokio::test]
async fn a_legacy_epochless_jwt_is_valid_at_epoch_zero() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	let token = encode(
		&JwtHeader::default(),
		&serde_json::json!({
			"sub": 1,
			"github_id": 101,
			"github_login": "user-1",
			"exp": (chrono::Utc::now().timestamp() + 600) as usize,
			"iat": 1,
		}),
		&EncodingKey::from_secret(state.jwt_secret.as_bytes()),
	)
	.unwrap();

	let principal = resolve_parts(&state, &HeaderMap::new(), &cookie_jar(&token))
		.await
		.unwrap();
	assert!(matches!(principal, Principal::Browser(_)));
}

#[tokio::test]
async fn forged_cookies_are_invalid_and_stale_ones_are_stale() {
	let state = state().await;
	insert_user(&state, 1, 101).await;

	let wrong_epoch = browser_token(&state, 1);
	assert!(matches!(
		resolve_parts(&state, &HeaderMap::new(), &cookie_jar(&wrong_epoch))
			.await
			.unwrap(),
		Principal::Stale
	));
	assert_eq!(
		resolve_parts(&state, &HeaderMap::new(), &cookie_jar(&wrong_epoch))
			.await
			.unwrap()
			.user_id(),
		None
	);

	sqlx::query("UPDATE users SET session_epoch = 1 WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();
	let old_epoch = browser_token(&state, 0);
	assert!(matches!(
		resolve_parts(&state, &HeaderMap::new(), &cookie_jar(&old_epoch))
			.await
			.unwrap(),
		Principal::Stale
	));
	assert_eq!(
		resolve_parts(&state, &HeaderMap::new(), &cookie_jar(&old_epoch))
			.await
			.unwrap()
			.user_id(),
		None
	);

	assert!(matches!(
		resolve_parts(&state, &HeaderMap::new(), &cookie_jar("forged"))
			.await
			.unwrap(),
		Principal::Invalid
	));
	assert!(
		resolve_parts(&state, &HeaderMap::new(), &cookie_jar("forged"))
			.await
			.unwrap()
			.session_user()
			.is_none()
	);
}

#[tokio::test]
async fn a_client_token_resolves_to_the_same_user() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	let session = client_sessions::create_session(&state.db, 1, "extension", 0, "mcd_token")
		.await
		.unwrap();

	let principal = resolve_parts(&state, &bearer_headers("mcd_token"), &CookieJar::new())
		.await
		.unwrap();
	let Principal::Client(user) = principal else {
		panic!("expected a client principal");
	};
	assert_eq!(user.session_id, session.id);
	assert_eq!(user.user_id, 1);
	assert_eq!(user.github_id, 101);
	assert_eq!(user.client, ClientKind::Extension);
}

#[tokio::test]
async fn a_bearer_wins_over_a_cookie_and_never_falls_back() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_token")
		.await
		.unwrap();
	let cookie = browser_token(&state, 0);

	let mut headers = bearer_headers("mcd_token");
	let cookies = cookie_jar(&cookie);
	let principal = resolve_parts(&state, &headers, &cookies).await.unwrap();
	assert!(matches!(principal, Principal::Client(_)));

	headers.insert(header::AUTHORIZATION, "Bearer wrong-token".parse().unwrap());
	assert!(matches!(
		resolve_parts(&state, &headers, &cookies).await.unwrap(),
		Principal::Invalid
	));

	headers.insert(header::AUTHORIZATION, "Basic wrong".parse().unwrap());
	assert!(matches!(
		resolve_parts(&state, &headers, &cookies).await.unwrap(),
		Principal::Invalid
	));
}

#[tokio::test]
async fn expired_revoked_and_deleted_clients_are_invalid() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_expired")
		.await
		.unwrap();
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_revoked")
		.await
		.unwrap();
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_deleted")
		.await
		.unwrap();
	sqlx::query("UPDATE client_sessions SET expires_at = ? WHERE token_hash = ?")
		.bind(chrono::Utc::now().timestamp() - 1)
		.bind(client_sessions::hash_secret("mcd_expired"))
		.execute(&state.db)
		.await
		.unwrap();
	client_sessions::revoke_session(&state.db, "mcd_revoked")
		.await
		.unwrap();
	sqlx::query("DELETE FROM users WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();

	for token in ["mcd_expired", "mcd_revoked", "mcd_deleted"] {
		assert!(
			matches!(
				resolve_parts(&state, &bearer_headers(token), &CookieJar::new())
					.await
					.unwrap(),
				Principal::Invalid
			),
			"{token} should not resolve"
		);
	}
}

#[tokio::test]
async fn malformed_client_headers_never_authorize_or_fall_back() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_known")
		.await
		.unwrap();
	// A stored row with a client kind the wire contract does not define
	// cannot become a principal either.
	client_sessions::create_session(&state.db, 1, "web", 0, "mcd_web")
		.await
		.unwrap();

	// A non-UTF-8 Authorization value cannot even be read as a scheme.
	let mut headers = HeaderMap::new();
	headers.insert(
		header::AUTHORIZATION,
		axum::http::HeaderValue::from_bytes(&[0xFF]).unwrap(),
	);
	assert!(matches!(
		resolve_parts(&state, &headers, &CookieJar::new())
			.await
			.unwrap(),
		Principal::Invalid
	));

	// A bare scheme without a token is rejected, not anonymous.
	let mut headers = HeaderMap::new();
	headers.insert(header::AUTHORIZATION, "Bearer ".parse().unwrap());
	assert!(matches!(
		resolve_parts(&state, &headers, &CookieJar::new())
			.await
			.unwrap(),
		Principal::Invalid
	));

	// The unknown-kind token resolves to nothing even though its hash matches.
	let mut headers = HeaderMap::new();
	headers.insert(header::AUTHORIZATION, "Bearer mcd_web".parse().unwrap());
	assert!(matches!(
		resolve_parts(&state, &headers, &CookieJar::new())
			.await
			.unwrap(),
		Principal::Invalid
	));

	// And the cookie behind those rejected headers is not consulted.
	let mut headers = HeaderMap::new();
	headers.insert(header::AUTHORIZATION, "Bearer mcd_web".parse().unwrap());
	headers.insert(
		header::COOKIE,
		format!("{SESSION_COOKIE_NAME}={}", browser_token(&state, 0))
			.parse()
			.unwrap(),
	);
	assert!(matches!(
		resolve_parts(&state, &headers, &CookieJar::new())
			.await
			.unwrap(),
		Principal::Invalid
	));
}

#[tokio::test]
async fn no_credentials_is_anonymous() {
	let state = state().await;
	let principal = resolve_parts(&state, &HeaderMap::new(), &CookieJar::new())
		.await
		.unwrap();
	assert!(matches!(principal, Principal::Anonymous));
	assert_eq!(principal.user_id(), None);
	assert!(principal.session_user().is_none());
}

async fn middleware_echo(request: axum::extract::Request) -> String {
	match request.extensions().get::<Principal>() {
		Some(Principal::Browser(user)) => format!("browser:{}", user.user_id),
		Some(Principal::Client(user)) => format!("client:{}", user.user_id),
		Some(Principal::Invalid) => "invalid".to_owned(),
		_ => "anonymous".to_owned(),
	}
}

fn middleware_router(state: Arc<AppState>) -> axum::Router {
	use axum::routing::get;
	use axum::routing::post;

	axum::Router::new()
		.route("/api/get_session", post(middleware_echo))
		.route("/health", get(middleware_echo))
		.route("/dashboard", get(middleware_echo))
		.layer(axum::middleware::from_fn_with_state(
			state,
			crate::sessions::principal_middleware,
		))
}

#[tokio::test]
async fn the_principal_middleware_resolves_each_request_once() {
	let state = state().await;
	insert_user(&state, 1, 101).await;
	client_sessions::create_session(&state.db, 1, "native", 0, "mcd_middleware")
		.await
		.unwrap();

	let send = |method: &'static str, headers: &HeaderMap, path: &'static str| {
		let router = middleware_router(Arc::new(state.clone()));
		let mut request = axum::extract::Request::builder()
			.method(method)
			.uri(path)
			.body(axum::body::Body::empty())
			.unwrap();
		*request.headers_mut() = headers.clone();
		async move {
			let response = router.oneshot(request).await.unwrap();
			axum::body::to_bytes(response.into_body(), 64)
				.await
				.unwrap()
		}
	};

	let mut browser_headers = HeaderMap::new();
	browser_headers.insert(
		header::COOKIE,
		format!("{}={}", SESSION_COOKIE_NAME, browser_token(&state, 0))
			.parse()
			.unwrap(),
	);
	let body = send("POST", &browser_headers, "/api/get_session").await;
	assert_eq!(&body[..], b"browser:1");

	let bearer = bearer_headers("mcd_middleware");
	let body = send("POST", &bearer, "/api/get_session").await;
	assert_eq!(&body[..], b"client:1");

	let mut invalid = HeaderMap::new();
	invalid.insert(header::AUTHORIZATION, "Bearer unknown".parse().unwrap());
	let body = send("POST", &invalid, "/api/get_session").await;
	assert_eq!(&body[..], b"invalid");

	let body = send("POST", &HeaderMap::new(), "/api/get_session").await;
	assert_eq!(&body[..], b"anonymous");

	// SSR page paths resolve the same credentials as the API surface: a
	// signed-in cookie renders signed in and a bearer stays a client.
	let body = send("GET", &browser_headers, "/dashboard").await;
	assert_eq!(&body[..], b"browser:1");
	let body = send("GET", &bearer, "/dashboard").await;
	assert_eq!(&body[..], b"client:1");
	let body = send("GET", &HeaderMap::new(), "/dashboard").await;
	assert_eq!(&body[..], b"anonymous");
	let body = send("GET", &bearer, "/health").await;
	assert_eq!(&body[..], b"client:1");
}

#[tokio::test]
async fn a_resolution_outage_answers_503_and_never_calls_downstream() {
	use std::sync::atomic::AtomicBool;
	use std::sync::atomic::Ordering;

	let _subscriber = tracing::subscriber::set_default(
		tracing_subscriber::fmt()
			.with_max_level(tracing::Level::ERROR)
			.with_test_writer()
			.finish(),
	);
	let state = state().await;
	insert_user(&state, 1, 101).await;

	// Take the database away: every credential resolution now fails.
	state.db.close().await;

	let reached = Arc::new(AtomicBool::new(false));
	let marker = reached.clone();
	let router = axum::Router::new()
		.route(
			"/dashboard",
			axum::routing::get(move || {
				marker.store(true, Ordering::SeqCst);
				async { "dashboard" }
			}),
		)
		.layer(axum::middleware::from_fn_with_state(
			Arc::new(state.clone()),
			crate::sessions::principal_middleware,
		));

	let send = |headers: HeaderMap, router: axum::Router| {
		async move {
			let mut request = axum::extract::Request::builder()
				.method("GET")
				.uri("/dashboard")
				.body(axum::body::Body::empty())
				.unwrap();
			*request.headers_mut() = headers;
			router.oneshot(request).await.unwrap()
		}
	};

	// A presented cookie must not be read as anonymous or invalid: the
	// outage is an unavailable service, and downstream never runs.
	let mut cookie_headers = HeaderMap::new();
	cookie_headers.insert(
		header::COOKIE,
		format!("{}={}", SESSION_COOKIE_NAME, browser_token(&state, 0))
			.parse()
			.unwrap(),
	);
	let response = send(cookie_headers, router.clone()).await;
	assert_eq!(
		response.status(),
		axum::http::StatusCode::SERVICE_UNAVAILABLE
	);
	assert!(!reached.load(Ordering::SeqCst));

	// A bearer outage is equally unavailable.
	let response = send(bearer_headers("mcd_any"), router.clone()).await;
	assert_eq!(
		response.status(),
		axum::http::StatusCode::SERVICE_UNAVAILABLE
	);
	assert!(!reached.load(Ordering::SeqCst));

	// A request with no credentials never touches the database and still
	// reaches downstream as anonymous.
	let response = send(HeaderMap::new(), router).await;
	assert_eq!(response.status(), axum::http::StatusCode::OK);
	assert!(reached.load(Ordering::SeqCst));
}
