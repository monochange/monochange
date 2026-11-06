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
use leptos_axum::ResponseOptions;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::create_token;
use monochange_app_api::github_app::GitHubAppAuth;
use monochange_app_api::oauth;
use rsa::RsaPrivateKey;
use rsa::pkcs1::EncodeRsaPrivateKey;
use rsa::pkcs1::LineEnding;
use rsa::rand_core::OsRng;

use super::PermissionUpdate;
use super::RepositoryConnectionStatus;
use super::permission_updates;
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
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "connection-test-signing-key",
		"token_encryption_key": "test-token-encryption-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
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
		state.github_api_origin = server.base_url();
	}

	Arc::new(state)
}

fn context(state: Arc<AppState>, cookie: Option<String>) -> (Owner, ResponseOptions) {
	let owner = Owner::new();
	let response = ResponseOptions::default();
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
		provide_context(response.clone());
	});
	(owner, response)
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
	let (owner, _) = context(state(Some(&server)).await, None);

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
	let (owner, response) = context(state, Some(cookie));

	let RepositoryConnectionStatus::Available { installation_url } = owner
		.with(|| ScopedFuture::new(repository_connection()))
		.await
		.unwrap()
	else {
		panic!("configured app should be available");
	};
	let installation_url = url::Url::parse(&installation_url).unwrap();
	assert_eq!(
		&installation_url[..url::Position::AfterPath],
		"https://github.com/apps/test-app/installations/new"
	);
	let nonce = installation_url
		.query_pairs()
		.find(|(name, _)| name == "state")
		.unwrap()
		.1
		.into_owned();
	let set_cookie = response.0.read().unwrap().headers[axum::http::header::SET_COOKIE]
		.to_str()
		.unwrap()
		.to_string();
	let oauth_cookie = axum_extra::extract::cookie::Cookie::parse_encoded(set_cookie)
		.unwrap()
		.into_owned();

	let verified = oauth::verify_login_state(
		"connection-test-signing-key",
		&axum_extra::extract::cookie::CookieJar::new().add(oauth_cookie),
		&nonce,
	)
	.unwrap();
	assert_eq!(verified.intent, oauth::OAuthIntent::Install);
	assert!(verified.code_verifier.is_none());
	metadata.assert();
}

#[tokio::test]
async fn unconfigured_app_is_explicitly_unavailable_to_a_signed_in_visitor() {
	let state = state(None).await;
	let cookie = session(&state, 1);
	let (owner, _) = context(state, Some(cookie));

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
	let (owner, _) = context(state, Some(cookie));
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
	let (owner, _) = context(
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
	let (owner, _) = context(state, Some(cookie));

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
	let (owner, _) = context(state, Some(cookie));
	let error = owner
		.with(|| ScopedFuture::new(repository_connection()))
		.await
		.unwrap_err()
		.to_string();

	assert!(error.contains("could not be loaded"));
	metadata.assert();
}

// ── Permission updates ──

async fn connect(state: &AppState, installation_id: i64, login: &str, account_type: &str) {
	let installation: i32 = sqlx::query_scalar(
		"INSERT INTO installations (user_id, github_installation_id, github_account_login, github_account_type, target_type)
		 VALUES (1, $1, $2, $3, 'selected') RETURNING id",
	)
	.bind(installation_id)
	.bind(login)
	.bind(account_type)
	.fetch_one(&state.db)
	.await
	.unwrap();

	let mut connection = state.db.acquire().await.unwrap();
	monochange_app_db::projects::link_installation_organization(
		&mut connection,
		installation,
		&monochange_app_db::projects::AccountIdentity {
			provider: monochange_app_db::projects::GITHUB.to_owned(),
			external_id: installation_id,
			login: login.to_owned(),
			account_type: account_type.to_owned(),
			avatar_url: None,
		},
	)
	.await
	.unwrap();
	drop(connection);

	for repository in ["one", "two"] {
		sqlx::query(
			"INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private, created_at, updated_at)
			 VALUES ($1, $2, $3, 0, '2026-10-20T00:00:00Z', '2026-10-20T00:00:00Z')",
		)
		.bind(installation)
		.bind(installation_id * 10 + i64::from(repository == "two"))
		.bind(format!("{login}/{repository}"))
		.execute(&state.db)
		.await
		.unwrap();
	}
}

fn mock_app(server: &MockServer, status: u16) -> httpmock::Mock<'_> {
	server.mock(|when, then| {
		when.method(Method::GET).path("/app");
		then.status(status).json_body(serde_json::json!({
			"slug": "test-app",
			"permissions": {"actions": "write", "checks": "write", "contents": "write", "members": "read", "metadata": "read", "organization_projects": "read", "pull_requests": "write"},
		}));
	})
}

fn mock_installation<'a>(
	server: &'a MockServer,
	installation_id: i64,
	status: u16,
	permissions: &serde_json::Value,
) -> httpmock::Mock<'a> {
	server.mock(|when, then| {
		when.method(Method::GET)
			.path(format!("/app/installations/{installation_id}"));
		then.status(status).json_body(serde_json::json!({
			"id": installation_id,
			"account": {"id": installation_id, "login": "account", "type": "User"},
			"permissions": permissions,
		}));
	})
}

async fn updates(state: Arc<AppState>, signed_in: bool) -> Vec<PermissionUpdate> {
	let cookie = signed_in.then(|| session(&state, 1));
	let (owner, _) = context(state, cookie);
	owner
		.with(|| ScopedFuture::new(permission_updates()))
		.await
		.unwrap()
}

#[tokio::test]
async fn installations_behind_the_app_permissions_link_to_their_review_page() {
	let server = MockServer::start();
	let app = mock_app(&server, 200);
	let personal = mock_installation(
		&server,
		11,
		200,
		&serde_json::json!({"checks": "read", "contents": "write", "metadata": "read", "pull_requests": "read"}),
	);
	let organization = mock_installation(
		&server,
		22,
		200,
		&serde_json::json!({"actions": "write", "checks": "write", "contents": "write", "members": "read", "metadata": "read", "organization_projects": "read", "pull_requests": "write"}),
	);
	let membership = server.mock(|when, then| {
		when.method(Method::GET)
			.path("/user/memberships/orgs/acme")
			.header("authorization", "Bearer test-only-token");
		then.status(200).json_body(
			serde_json::json!({"state": "active", "role": "admin", "organization": {"id":22}}),
		);
	});
	let state = state(Some(&server)).await;
	connect(&state, 11, "alice", "User").await;
	connect(&state, 22, "acme", "Organization").await;

	assert_eq!(
		updates(state, true).await,
		vec![PermissionUpdate {
			account: "alice".to_string(),
			review_url: "https://github.com/settings/installations/11".to_string(),
			missing: "Actions: write (now none), Checks: write (now read), Pull requests: write (now read)"
				.to_string(),
		}],
	);
	app.assert_calls(1);
	personal.assert_calls(1);
	organization.assert_calls(1);
	membership.assert_calls(1);
}

#[tokio::test]
async fn organization_installations_review_on_the_organization_settings_page() {
	let server = MockServer::start();
	mock_app(&server, 200);
	mock_installation(&server, 22, 200, &serde_json::json!({"metadata": "read"}));
	server.mock(|when, then| {
		when.method(Method::GET).path("/user/memberships/orgs/acme");
		then.status(200).json_body(
			serde_json::json!({"state": "active", "role": "admin", "organization": {"id":22}}),
		);
	});
	let state = state(Some(&server)).await;
	connect(&state, 22, "acme", "Organization").await;

	let updates = updates(state, true).await;
	assert_eq!(updates.len(), 1);
	assert_eq!(
		updates[0].review_url,
		"https://github.com/organizations/acme/settings/installations/22"
	);
	assert!(
		updates[0].missing.contains("Members: read (now none)"),
		"{}",
		updates[0].missing
	);
}

#[tokio::test]
async fn failed_installation_lookups_leave_only_that_account_out() {
	let server = MockServer::start();
	mock_app(&server, 200);
	mock_installation(&server, 11, 404, &serde_json::json!({}));
	mock_installation(&server, 33, 200, &serde_json::json!({"metadata": "read"}));
	let state = state(Some(&server)).await;
	connect(&state, 11, "alice", "User").await;
	connect(&state, 33, "alice-two", "User").await;

	let updates = updates(state, true).await;
	assert_eq!(
		updates
			.iter()
			.map(|update| update.account.as_str())
			.collect::<Vec<_>>(),
		vec!["alice-two"]
	);
}

#[tokio::test]
async fn permission_updates_are_empty_when_nothing_can_be_checked() {
	// No configured app.
	let unconfigured = state(None).await;
	connect(&unconfigured, 11, "alice", "User").await;
	assert!(updates(unconfigured, true).await.is_empty());

	// Signed out, and signed in without installations, never ask GitHub.
	let server = MockServer::start();
	let app = mock_app(&server, 200);
	let signed_out = state(Some(&server)).await;
	connect(&signed_out, 11, "alice", "User").await;
	assert!(updates(signed_out, false).await.is_empty());
	assert!(updates(state(Some(&server)).await, true).await.is_empty());
	app.assert_calls(0);

	// A failed app lookup hides the notice instead of failing the dashboard.
	let failing = MockServer::start();
	let app = mock_app(&failing, 500);
	let installation = mock_installation(&failing, 11, 200, &serde_json::json!({}));
	let state = state(Some(&failing)).await;
	connect(&state, 11, "alice", "User").await;
	assert!(updates(state, true).await.is_empty());
	app.assert_calls(1);
	installation.assert_calls(0);
}
