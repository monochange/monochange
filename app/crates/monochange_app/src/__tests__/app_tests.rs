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

async fn dashboard_html(app: &Router, cookie: &str) -> String {
	let response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri("/dashboard")
				.header(COOKIE, cookie)
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::OK);
	html(response).await
}

async fn deliver_repository_event(app: &Router, event: &str, payload: serde_json::Value) {
	use hmac::Hmac;
	use hmac::Mac;
	use rsa::sha2::Sha256;

	let payload = payload.to_string();
	let mut mac = Hmac::<Sha256>::new_from_slice(b"test-only-webhook-secret").unwrap();
	mac.update(payload.as_bytes());
	let signature = format!("sha256={:x}", mac.finalize().into_bytes());
	let response = app
		.clone()
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/api/github/webhooks")
				.header("x-hub-signature-256", signature)
				.header("x-github-event", event)
				.body(Body::from(payload))
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn repository_lifecycle_updates_the_dashboard_and_keeps_connection_available() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/app").header_exists("authorization");
			then.json_body(serde_json::json!({"slug":"test-app"}));
		})
		.await;
	let state = configured_state(&server).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-alice-token'), (2, 202, 'bob', 'test-only-bob-token')").execute(&state.db).await.unwrap();
	let alice_token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let alice_cookie = format!("{}={alice_token}", oauth::SESSION_COOKIE_NAME);
	let bob_token = monochange_app_api::create_token(&state.jwt_secret, 2, 202, "bob").unwrap();
	let bob_cookie = format!("{}={bob_token}", oauth::SESSION_COOKIE_NAME);
	let app = router(state.clone()).merge(monochange_app_api::api_router((*state).clone()));
	let owner = serde_json::json!({"id":1001,"account":{"id":101,"login":"alice","type":"User"},"repository_selection":"all"});
	let empty = dashboard_html(&app, &alice_cookie).await;
	assert!(empty.contains("No repositories connected yet"));
	assert!(empty.contains("Connect repositories on GitHub"));
	assert!(empty.contains("href=\"https://github.com/apps/test-app/installations/new?state="));

	deliver_repository_event(&app, "installation", serde_json::json!({
		"action":"created", "installation":owner,
		"repositories":[{"id":11,"full_name":"alice/public","private":false},{"id":12,"full_name":"alice/private","private":true}]
	})).await;
	deliver_repository_event(
		&app,
		"installation",
		serde_json::json!({
			"action":"created", "installation":{"id":2002,"account":{"id":202,"login":"bob","type":"User"}},
			"repositories":[{"id":21,"full_name":"bob/private","private":true}]
		}),
	)
	.await;
	let connected = dashboard_html(&app, &alice_cookie).await;
	assert!(connected.contains("alice/public"));
	assert!(connected.contains("alice/private"));
	assert!(connected.contains("Connect repositories on GitHub"));
	assert!(!connected.contains("No repositories connected yet"));
	assert!(!connected.contains("bob/private"));
	let other_account = dashboard_html(&app, &bob_cookie).await;
	assert!(other_account.contains("bob/private"));
	assert!(!other_account.contains("alice/private"));

	deliver_repository_event(
		&app,
		"installation_repositories",
		serde_json::json!({
			"action":"added", "installation":owner,
			"repositories_added":[{"id":13,"full_name":"alice/added-later","private":true}]
		}),
	)
	.await;
	let added = dashboard_html(&app, &alice_cookie).await;
	assert!(added.contains("alice/added-later"));
	assert!(added.contains("alice/public"));
	assert!(added.contains("Connect repositories on GitHub"));

	deliver_repository_event(
		&app,
		"installation_repositories",
		serde_json::json!({
			"action":"removed", "installation":owner,
			"repositories_removed":[{"id":11,"full_name":"alice/public","private":false}]
		}),
	)
	.await;
	let selected = dashboard_html(&app, &alice_cookie).await;
	assert!(!selected.contains("alice/public"));
	assert!(selected.contains("alice/private"));
	assert!(selected.contains("alice/added-later"));
	assert!(selected.contains("Connect repositories on GitHub"));

	for (action, suspended) in [("suspend", true), ("unsuspend", false)] {
		deliver_repository_event(
			&app,
			"installation",
			serde_json::json!({
				"action":action, "installation":owner
			}),
		)
		.await;
		let page = dashboard_html(&app, &alice_cookie).await;
		assert_eq!(page.contains("Installation suspended"), suspended);
		assert!(page.contains("alice/added-later"));
		assert!(page.contains("Connect repositories on GitHub"));
	}

	deliver_repository_event(
		&app,
		"installation",
		serde_json::json!({
			"action":"deleted", "installation":owner
		}),
	)
	.await;
	let removed = dashboard_html(&app, &alice_cookie).await;
	assert!(removed.contains("No repositories connected yet"));
	assert!(removed.contains("Connect repositories on GitHub"));
	assert!(!removed.contains("alice/private"));
	assert!(!removed.contains("alice/added-later"));
	assert!(
		dashboard_html(&app, &bob_cookie)
			.await
			.contains("bob/private")
	);
}

#[tokio::test]
async fn dashboard_rechecks_organization_ownership_and_reports_expired_authorization() {
	let server = MockServer::start_async().await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/app");
			then.json_body(serde_json::json!({"slug":"test-app"}));
		})
		.await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/acme");
			then.json_body(serde_json::json!({"state":"active","role":"admin"}));
		})
		.await;
	let state = configured_state(&server).await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-alice-token')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'acme', 'Organization')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'acme/private', 1)").execute(&state.db).await.unwrap();
	let token = monochange_app_api::create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let cookie = format!("{}={token}", oauth::SESSION_COOKIE_NAME);
	let app = router(state);
	assert!(dashboard_html(&app, &cookie).await.contains("acme/private"));
	membership.delete_async().await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/acme");
			then.json_body(serde_json::json!({"state":"active","role":"member"}));
		})
		.await;
	let revoked = dashboard_html(&app, &cookie).await;
	assert!(!revoked.contains("acme/private"));
	assert!(revoked.contains("No repositories connected yet"));
	assert!(revoked.contains("Connect repositories on GitHub"));
	membership.delete_async().await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/acme");
			then.json_body(serde_json::json!({"state":"active","role":"admin"}));
		})
		.await;
	let restored = dashboard_html(&app, &cookie).await;
	assert!(restored.contains("acme/private"));
	assert!(restored.contains("Connect repositories on GitHub"));
	membership.delete_async().await;
	server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/acme");
			then.status(401)
				.body("test-only-sensitive-provider-response");
		})
		.await;
	let expired = dashboard_html(&app, &cookie).await;
	assert!(expired.contains("Repositories couldn't be loaded"));
	assert!(expired.contains("Sign in with GitHub"));
	assert!(!expired.contains("acme/private"));
	assert!(!expired.contains("No repositories connected yet"));
	assert!(!expired.contains("test-only-sensitive-provider-response"));
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
async fn public_navigation_names_the_changelog_in_desktop_mobile_and_footer_links() {
	let server = MockServer::start_async().await;
	let response = router(state(&server).await)
		.oneshot(
			Request::builder()
				.uri("/install")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(response.status(), StatusCode::OK);
	let body = html(response).await;
	let named_links = body
		.split("href=\"/changelog\"")
		.skip(1)
		.filter(|link| link.split("</a>").next().unwrap().contains("Changelog"))
		.count();
	assert_eq!(named_links, 3);
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
	let verified = oauth::verify_login_state(&state.jwt_secret, &jar, &nonce).unwrap();
	assert_eq!(verified.intent, oauth::OAuthIntent::Login);
	assert!(verified.code_verifier.is_some());
	assert!(
		url.query_pairs()
			.any(|(key, value)| key == "code_challenge_method" && value == "S256")
	);
	assert!(url.query_pairs().any(|(key, value)| {
		key == "redirect_uri" && value == "https://monochange.dev/auth/callback"
	}));
	assert!(!url.query_pairs().any(|(key, _)| key == "scope"));
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
	assert!(body.contains("href=\"https://github.com/apps/test-app/installations/new?state="));
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
			when.method(POST)
				.path("/login/oauth/access_token")
				.body_includes("code_verifier=")
				.body_includes("redirect_uri=");
			then.delay(Duration::from_millis(50))
				.json_body(serde_json::json!({
					"access_token":"test-only-access-token",
					"expires_in":28800,
					"refresh_token":"test-only-refresh-token",
					"refresh_token_expires_in":15_897_600
				}));
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
	let pending = oauth::login_state(&state.jwt_secret).unwrap();
	let app = router(state.clone());
	let response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri(format!(
					"/auth/callback?code=test-only-code&state={}",
					pending.state
				))
				.header(ACCEPT, "text/html")
				.header(COOKIE, pending.cookie.to_string())
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
	let stored: (String, String, i64, i64) = sqlx::query_as(
		"SELECT github_access_token, github_refresh_token,
		        github_access_token_expires_at, github_refresh_token_expires_at
		 FROM users WHERE github_id = 101",
	)
	.fetch_one(&state.db)
	.await
	.unwrap();
	assert_eq!(stored.0, "test-only-access-token");
	assert_eq!(stored.1, "test-only-refresh-token");
	assert!(stored.2 > chrono::Utc::now().timestamp());
	assert!(stored.3 > stored.2);
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
		let pending = oauth::login_state(&state.jwt_secret).unwrap();
		Request::builder()
			.uri(format!(
				"/auth/callback?code=test-only-code&state={}",
				pending.state
			))
			.header(ACCEPT, "text/html")
			.header(COOKIE, pending.cookie.to_string())
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
	let pending = oauth::login_state(&state.jwt_secret).unwrap();
	let response = router(state.clone())
		.oneshot(
			Request::builder()
				.uri(format!(
					"/auth/callback?code=expired-test-code&state={}",
					pending.state
				))
				.header(COOKIE, pending.cookie.to_string())
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
