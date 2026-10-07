//! Exercise callback cookie delivery and subsequent navigation through real SSR responses.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::body::to_bytes;
use axum::http::Request;
use axum::http::StatusCode;
use axum::http::header::ACCEPT;
use axum::http::header::COOKIE;
use axum::http::header::LOCATION;
use axum::http::header::SET_COOKIE;
use axum_extra::extract::cookie::Cookie;
use httpmock::Method::GET;
use httpmock::Method::POST;
use httpmock::MockServer;
use leptos::prelude::*;
use leptos_axum::LeptosRoutes;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::oauth;
use tower::ServiceExt;

async fn html(response: axum::response::Response) -> String {
	let bytes = tokio::time::timeout(
		Duration::from_secs(10),
		to_bytes(response.into_body(), 2_000_000),
	)
	.await
	.expect("SSR must finish its response stream")
	.unwrap();
	String::from_utf8(bytes.to_vec()).unwrap()
}

async fn state(server: &MockServer) -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"jwt_secret": "callback-test-signing-key",
		"github_client_id": "test-client",
		"github_client_secret": "test-only-client-secret"
	}))
	.unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_oauth_origin = server.base_url();
	state.github_api_origin = server.base_url();
	Arc::new(state)
}

async fn configured_state(server: &MockServer) -> Arc<AppState> {
	use monochange_app_api::github_app::GitHubAppAuth;
	use rsa::RsaPrivateKey;
	use rsa::pkcs1::EncodeRsaPrivateKey;
	use rsa::pkcs1::LineEnding;
	use rsa::rand_core::OsRng;

	static TEST_KEY: OnceLock<String> = OnceLock::new();
	let key = TEST_KEY.get_or_init(|| {
		RsaPrivateKey::new(&mut OsRng, 2048)
			.unwrap()
			.to_pkcs1_pem(LineEnding::LF)
			.unwrap()
			.to_string()
	});
	let mut state = state(server).await;
	Arc::get_mut(&mut state).unwrap().github_app = Some(GitHubAppAuth::new(
		"123",
		key,
		"test-only-webhook-secret",
		&server.base_url(),
	));
	state
}

fn router(state: Arc<AppState>) -> Router {
	let options = LeptosOptions::builder()
		.output_name("monochange_app")
		.site_root(
			std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
				.join("../../../target/site")
				.to_string_lossy()
				.into_owned(),
		)
		.build();
	let routes = crate::tests::routes();
	let request_state = state.clone();
	Router::<LeptosOptions>::new()
		.leptos_routes_with_context(
			&options,
			routes,
			move || {
				provide_context(request_state.clone());
			},
			{
				let options = options.clone();
				move || crate::app::shell(options.clone())
			},
		)
		.fallback(leptos_axum::file_and_error_handler(
			move |options: LeptosOptions| {
				provide_context(state.clone());
				provide_context(options.clone());
				crate::app::shell(options)
			},
		))
		.with_state(options)
}

#[tokio::test]
async fn unknown_route_renders_an_actionable_not_found_page() {
	let server = MockServer::start_async().await;
	let response = router(state(&server).await)
		.oneshot(
			Request::builder()
				.uri("/missing-test-page")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::NOT_FOUND);
	let body = html(response).await;
	assert!(body.contains("Page not found"));
	assert!(body.contains("href=\"/\""));
}

#[tokio::test]
async fn login_delivers_browser_state_before_github_authorization_link() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let response = router(state.clone())
		.oneshot(
			Request::builder()
				.uri("/login")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::OK);
	let cookie =
		Cookie::parse_encoded(response.headers()[SET_COOKIE].to_str().unwrap().to_string())
			.unwrap();
	assert_eq!(cookie.name(), oauth::OAUTH_COOKIE_NAME);
	assert_eq!(cookie.http_only(), Some(true));
	assert_eq!(cookie.secure(), Some(true));
	let body = html(response).await;
	let authorize = body
		.split("href=\"")
		.find(|part| part.starts_with(&format!("{}/login/oauth/authorize", server.base_url())))
		.unwrap()
		.split('"')
		.next()
		.unwrap()
		.replace("&amp;", "&");
	let url = reqwest::Url::parse(&authorize).unwrap();
	let nonce = url
		.query_pairs()
		.find(|(key, _)| key == "state")
		.unwrap()
		.1
		.into_owned();
	let jar = axum_extra::extract::cookie::CookieJar::new().add(cookie.into_owned());
	oauth::verify_login_state(&state.jwt_secret, &jar, &nonce).unwrap();
	assert_eq!(
		url.query_pairs().find(|(key, _)| key == "scope").unwrap().1,
		"user:email,read:org"
	);
}

#[tokio::test]
async fn invalid_session_dashboard_offers_sign_in_again_without_protected_data() {
	let server = MockServer::start_async().await;
	let response = router(state(&server).await)
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.header(
					COOKIE,
					format!("{}=invalid-token", oauth::SESSION_COOKIE_NAME),
				)
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let body = html(response).await;
	assert!(body.contains("Sign in again"));
	assert!(body.contains("Your session couldn't be loaded"));
	assert!(!body.contains("Sign out"));
	assert!(!body.contains("Connect repositories on GitHub"));
}

#[tokio::test]
async fn configured_dashboard_connects_to_verified_app_and_shows_suspended_repository() {
	let server = MockServer::start_async().await;
	let metadata = server
		.mock_async(|when, then| {
			when.method(GET).path("/app").header_exists("authorization");
			then.json_body(serde_json::json!({"slug":"test-app"}));
		})
		.await;
	let state = configured_state(&server).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type, target_type) VALUES (1, 1, 1001, 'alice', 'User', 'suspended')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'alice/private', 1)").execute(&state.db).await.unwrap();
	let token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let response = router(state)
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.header(COOKIE, format!("{}={token}", oauth::SESSION_COOKIE_NAME))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let body = html(response).await;
	assert!(body.contains("href=\"https://github.com/apps/test-app/installations/new\""));
	assert!(body.contains("Connect repositories on GitHub"));
	assert!(body.contains("alice/private"));
	assert!(body.contains("Installation suspended"));
	assert!(!body.contains("Repository connection is unavailable"));
	metadata.assert_async().await;
}

#[tokio::test]
async fn failed_app_metadata_dashboard_shows_retry_without_unverified_install_link() {
	let server = MockServer::start_async().await;
	let metadata = server
		.mock_async(|when, then| {
			when.method(GET).path("/app");
			then.status(503).body("test-only-sensitive-response");
		})
		.await;
	let state = configured_state(&server).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token')").execute(&state.db).await.unwrap();
	let token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let response = router(state)
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.header(COOKIE, format!("{}={token}", oauth::SESSION_COOKIE_NAME))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let body = html(response).await;
	assert!(body.contains("Repository setup couldn't be loaded"));
	assert!(body.contains("Try again"));
	assert!(!body.contains("test-only-sensitive-response"));
	assert!(!body.contains("Connect repositories on GitHub"));
	metadata.assert_async().await;
}

#[tokio::test]
async fn callback_delivers_session_cookie_before_showing_success_and_dashboard_uses_it() {
	let server = MockServer::start_async().await;
	let token = server
		.mock_async(|when, then| {
			when.method(POST).path("/login/oauth/access_token");
			then.delay(Duration::from_millis(50))
				.json_body(serde_json::json!({"access_token":"test-only-access-token"}));
		})
		.await;
	let user = server
		.mock_async(|when, then| {
			when.method(GET)
				.path("/user")
				.header("authorization", "Bearer test-only-access-token");
			then.json_body(serde_json::json!({"id":101, "login":"alice", "avatar_url":null}));
		})
		.await;
	let state = state(&server).await;
	let (nonce, cookie) = oauth::login_state(&state.jwt_secret).unwrap();
	let app = router(state);
	let response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri(format!("/auth/callback?code=test-only-code&state={nonce}"))
				.header(ACCEPT, "text/html")
				.header(COOKIE, cookie.to_string())
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::FOUND);
	assert_eq!(response.headers()[LOCATION], "/dashboard");
	let cookies: Vec<_> = response
		.headers()
		.get_all(SET_COOKIE)
		.iter()
		.map(|value| {
			Cookie::parse_encoded(value.to_str().unwrap().to_string())
				.unwrap()
				.into_owned()
		})
		.collect();
	let body = html(response).await;
	assert!(body.contains("Signed in!"), "callback did not succeed");
	token.assert_async().await;
	user.assert_async().await;
	let session = cookies
		.iter()
		.find(|cookie| cookie.name() == oauth::SESSION_COOKIE_NAME)
		.expect("successful callback must deliver the session cookie in the HTTP headers");
	assert_eq!(session.secure(), Some(true));
	assert_eq!(session.http_only(), Some(true));
	assert_eq!(session.path(), Some("/"));
	assert_eq!(session.domain(), None);
	let response = app
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.header(COOKIE, format!("{}={}", session.name(), session.value()))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let body = html(response).await;
	assert!(body.contains("Welcome, "));
	assert!(body.contains("alice"));
	assert!(body.contains("No repositories connected yet"));
	assert!(!body.contains("Your workspace starts here."));
}

#[tokio::test]
async fn signed_in_dashboard_navigation_offers_dashboard_and_sign_out() {
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token')").execute(&state.db).await.unwrap();
	let token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let response = router(state)
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.header(COOKIE, format!("{}={token}", oauth::SESSION_COOKIE_NAME))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let body = html(response).await;
	assert!(body.contains("Welcome, "));
	assert!(
		body.contains("Sign out"),
		"authenticated navigation must offer sign out"
	);
	assert!(body.contains("Dashboard"));
	assert!(!body.contains(">Sign in<"));
}

#[tokio::test]
async fn simultaneous_callbacks_share_one_identity_and_preserve_an_existing_workspace() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(POST).path("/login/oauth/access_token");
			then.delay(Duration::from_millis(50))
				.json_body(serde_json::json!({"access_token":"updated-test-token"}));
		})
		.await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/user");
			then.json_body(
				serde_json::json!({"id":101,"login":"alice","avatar_url":"https://avatars.example/alice"}),
			);
		})
		.await;
	let state = state(&server).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token, plan_tier) VALUES (7, 101, 'old-alice', '', 'team')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO installations (user_id, github_installation_id, github_account_login, github_account_type) VALUES (7, 1001, 'alice', 'User')").execute(&state.db).await.unwrap();
	let request = || {
		let (nonce, cookie) = oauth::login_state(&state.jwt_secret).unwrap();
		Request::builder()
			.uri(format!("/auth/callback?code=test-only-code&state={nonce}"))
			.header(ACCEPT, "text/html")
			.header(COOKIE, cookie.to_string())
			.body(Body::empty())
			.unwrap()
	};
	let app = router(state.clone());
	let (first, second) = tokio::join!(app.clone().oneshot(request()), app.oneshot(request()));
	assert_eq!(first.unwrap().status(), StatusCode::FOUND);
	assert_eq!(second.unwrap().status(), StatusCode::FOUND);
	let identity: (i32, String, String) = sqlx::query_as(
		"SELECT id, plan_tier, github_access_token FROM users WHERE github_id = 101",
	)
	.fetch_one(&state.db)
	.await
	.unwrap();
	assert_eq!(
		identity,
		(7, "team".to_string(), "updated-test-token".to_string())
	);
	assert_eq!(
		sqlx::query_scalar::<_, i32>(
			"SELECT user_id FROM installations WHERE github_installation_id = 1001"
		)
		.fetch_one(&state.db)
		.await
		.unwrap(),
		7
	);
}

#[tokio::test]
async fn callback_failure_does_not_create_a_session_and_consumes_the_pending_login() {
	let server = MockServer::start_async().await;
	let exchange = server
		.mock_async(|when, then| {
			when.method(POST).path("/login/oauth/access_token");
			then.delay(Duration::from_millis(20))
				.json_body(serde_json::json!({"error":"bad_verification_code"}));
		})
		.await;
	let state = state(&server).await;
	let (nonce, cookie) = oauth::login_state(&state.jwt_secret).unwrap();
	let response = router(state.clone())
		.oneshot(
			Request::builder()
				.uri(format!(
					"/auth/callback?code=expired-test-code&state={nonce}"
				))
				.header(COOKIE, cookie.to_string())
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let cookies: Vec<_> = response
		.headers()
		.get_all(SET_COOKIE)
		.iter()
		.map(|value| {
			Cookie::parse_encoded(value.to_str().unwrap().to_string())
				.unwrap()
				.into_owned()
		})
		.collect();
	let body = html(response).await;
	assert!(body.contains("Sign in failed"));
	assert!(
		!cookies
			.iter()
			.any(|cookie| cookie.name() == oauth::SESSION_COOKIE_NAME)
	);
	assert!(cookies.iter().any(|cookie| {
		cookie.name() == oauth::OAUTH_COOKIE_NAME && cookie.max_age() == Some(time::Duration::ZERO)
	}));
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		0
	);
	exchange.assert_async().await;
}

#[tokio::test]
async fn callback_without_browser_state_cannot_exchange_a_code() {
	let server = MockServer::start_async().await;
	let outbound = server
		.mock_async(|when, then| {
			when.any_request();
			then.status(500);
		})
		.await;
	let response = router(state(&server).await)
		.oneshot(
			Request::builder()
				.uri("/auth/callback?code=must-not-be-used&state=wrong-state")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert!(!response.headers().get_all(SET_COOKIE).iter().any(|value| {
		value
			.to_str()
			.unwrap()
			.starts_with(oauth::SESSION_COOKIE_NAME)
	}));
	let body = html(response).await;
	assert!(body.contains("Sign in failed"));
	assert!(body.contains("Invalid or expired OAuth state"));
	outbound.assert_calls_async(0).await;
}

#[tokio::test]
async fn sign_out_expires_cookie_and_returns_to_public_site() {
	use server_fn::ServerFn;
	let server = MockServer::start_async().await;
	let state = state(&server).await;
	let token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let app = router(state);
	let response = app
		.clone()
		.oneshot(
			Request::builder()
				.method("POST")
				.uri(crate::server_fns::auth::Logout::PATH)
				.header(ACCEPT, "text/html")
				.header("content-type", "application/x-www-form-urlencoded")
				.header(COOKIE, format!("{}={token}", oauth::SESSION_COOKIE_NAME))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::FOUND);
	assert_eq!(response.headers()[LOCATION], "/");
	let cookie =
		Cookie::parse_encoded(response.headers()[SET_COOKIE].to_str().unwrap().to_string())
			.unwrap();
	assert_eq!(cookie.name(), oauth::SESSION_COOKIE_NAME);
	assert_eq!(cookie.max_age(), Some(time::Duration::ZERO));
	assert_eq!(cookie.secure(), Some(true));
	let response = app
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	let body = html(response).await;
	assert!(body.contains("Your workspace starts here."));
	assert!(!body.contains("Sign out"));
}

/// Manual browser harness: real app/cookies/database, fixture GitHub responses only.
#[tokio::test]
#[ignore = "starts a loopback server for the real Chrome critical-flow audit"]
async fn browser_flow_fixture_server() {
	let github = MockServer::start_async().await;
	github
		.mock_async(|when, then| {
			when.method(POST).path("/login/oauth/access_token");
			then.delay(Duration::from_millis(50))
				.json_body(serde_json::json!({"access_token":"test-only-access-token"}));
		})
		.await;
	github
		.mock_async(|when, then| {
			when.method(GET).path("/user");
			then.json_body(serde_json::json!({"id":101,"login":"alice","avatar_url":null}));
		})
		.await;
	github
		.mock_async(|when, then| {
			when.method(GET).path("/app");
			then.json_body(serde_json::json!({"slug":"test-app"}));
		})
		.await;
	let state = configured_state(&github).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'alice', 'User')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'alice/public', 0), (1, 12, 'alice/private', 1)").execute(&state.db).await.unwrap();
	let site = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/site");
	let app = router(state)
		.merge(crate::public_routes::book_redirects())
		.route_service(
			"/pkg/monochange_app_bg.wasm",
			tower_http::services::ServeFile::new(site.join("pkg/monochange_app.wasm")),
		);
	let listener = tokio::net::TcpListener::bind("127.0.0.1:3103")
		.await
		.unwrap();
	println!(
		"LOCAL_BROWSER_FIXTURE http://127.0.0.1:3103; GitHub fixture origin {}",
		github.base_url()
	);
	axum::serve(listener, app).await.unwrap();
}
