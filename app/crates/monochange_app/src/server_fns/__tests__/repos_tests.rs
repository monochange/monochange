//! Repository access uses the same authenticated session as the dashboard.

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::disallowed_methods)]

use std::sync::Arc;

use axum::http::Request;
use axum::http::header::COOKIE;
use httpmock::Method::GET;
use httpmock::MockServer;
use leptos::prelude::*;
use leptos::reactive::computed::ScopedFuture;
use monochange_app_api::AppSecrets;
use monochange_app_api::AppState;
use monochange_app_api::create_token;
use monochange_app_api::oauth;

use super::get_repo;
use super::list_repos;

async fn state() -> Arc<AppState> {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (1, 101, 'alice', 'test-only-token'), (2, 202, 'bob', 'test-only-token')").execute(&db).await.unwrap();
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (1, 1, 1001, 'alice', 'User'), (2, 2, 1002, 'bob', 'User')").execute(&db).await.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (1, 11, 'alice/public', 0), (1, 12, 'alice/private', 1), (2, 21, 'bob/private', 1)").execute(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({
		"database_url": "sqlite::memory:",
		"jwt_secret": "repository-test-signing-key",
		"github_client_id": "",
		"github_client_secret": "",
	}))
	.unwrap();
	Arc::new(AppState::new(db, secrets).unwrap())
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
async fn signed_in_user_sees_own_public_and_private_repositories() {
	let state = state().await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let mut repos = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap();
	repos.sort_by(|a, b| a.github_full_name.cmp(&b.github_full_name));
	assert_eq!(repos.len(), 2);
	assert_eq!(repos[0].github_full_name, "alice/private");
	assert!(repos[0].github_private);
	assert_eq!(repos[1].github_full_name, "alice/public");
	assert!(!repos[1].github_private);
}

#[tokio::test]
async fn repository_lookup_cannot_reveal_another_users_repository() {
	let state = state().await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let own = owner
		.with(|| ScopedFuture::new(get_repo("alice/private".to_string())))
		.await
		.unwrap();
	assert_eq!(own.unwrap().github_full_name, "alice/private");
	assert!(
		owner
			.with(|| ScopedFuture::new(get_repo("bob/private".to_string())))
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn signed_out_user_cannot_list_connected_repositories() {
	let owner = context(state().await, None);
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn invalid_session_is_an_error_and_legacy_cookie_does_not_authorize_access() {
	let state = state().await;
	let owner = context(
		state.clone(),
		Some(format!("{}=invalid-token", oauth::SESSION_COOKIE_NAME)),
	);
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err()
			.to_string()
			.contains("Invalid session")
	);
	let token = create_token(&state.jwt_secret, 1, 101, "alice").unwrap();
	let owner = context(state, Some(format!("monochange_session={token}")));
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn suspended_installation_is_visible_as_suspended() {
	let state = state().await;
	sqlx::query("UPDATE installations SET target_type = 'suspended' WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let repos = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap();
	assert_eq!(repos.len(), 2);
	assert!(repos.iter().all(|repo| repo.installation_suspended));
}

async fn organization_state(server: &MockServer) -> Arc<AppState> {
	let state = state().await;
	sqlx::query("INSERT INTO installations (id, user_id, github_installation_id, github_account_login, github_account_type) VALUES (3, 1, 1003, 'team', 'Organization')").execute(&state.db).await.unwrap();
	sqlx::query("INSERT INTO repositories (installation_id, github_repo_id, github_full_name, github_private) VALUES (3, 31, 'team/private', 1), (3, 32, 'team/second', 1)").execute(&state.db).await.unwrap();
	let mut state = (*state).clone();
	state.github_api_origin = server.base_url();
	Arc::new(state)
}

#[tokio::test]
async fn organization_repositories_require_current_active_owner_membership() {
	let server = MockServer::start_async().await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET)
				.path("/user/memberships/orgs/team")
				.header("authorization", "Bearer test-only-token");
			then.json_body(serde_json::json!({"state":"active","role":"admin"}));
		})
		.await;
	let state = organization_state(&server).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let repositories = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap();
	assert_eq!(repositories.len(), 4);
	assert_eq!(
		repositories
			.iter()
			.filter(|repository| repository.installation_login == "team")
			.count(),
		2
	);
	membership.assert_calls_async(1).await;
}

#[tokio::test]
async fn one_user_token_checks_repositories_from_multiple_organizations() {
	let server = MockServer::start_async().await;
	let team_membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/team");
			then.json_body(serde_json::json!({"state":"active","role":"admin"}));
		})
		.await;
	let second_membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/second-team");
			then.json_body(serde_json::json!({"state":"active","role":"admin"}));
		})
		.await;
	let state = organization_state(&server).await;
	sqlx::query(
		"INSERT INTO installations (
			id, user_id, github_installation_id, github_account_login, github_account_type
		 ) VALUES (4, 1, 1004, 'second-team', 'Organization')",
	)
	.execute(&state.db)
	.await
	.unwrap();
	sqlx::query(
		"INSERT INTO repositories (
			installation_id, github_repo_id, github_full_name, github_private
		 ) VALUES (4, 41, 'second-team/private', 1)",
	)
	.execute(&state.db)
	.await
	.unwrap();
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let repositories = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap();

	assert_eq!(repositories.len(), 5);
	assert!(
		repositories
			.iter()
			.any(|repository| repository.github_full_name == "second-team/private")
	);
	team_membership.assert_calls_async(1).await;
	second_membership.assert_calls_async(1).await;
}

#[tokio::test]
async fn ordinary_or_pending_members_cannot_read_organization_repositories() {
	for (state_value, role) in [
		("active", "member"),
		("pending", "admin"),
		("pending", "member"),
	] {
		let server = MockServer::start_async().await;
		let membership = server
			.mock_async(|when, then| {
				when.method(GET).path("/user/memberships/orgs/team");
				then.json_body(serde_json::json!({"state":state_value,"role":role}));
			})
			.await;
		let state = organization_state(&server).await;
		let cookie = session(&state, 1);
		let owner = context(state, Some(cookie));
		let repositories = owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap();
		assert_eq!(repositories.len(), 2);
		assert!(
			repositories
				.iter()
				.all(|repository| repository.installation_login == "alice")
		);
		assert!(
			owner
				.with(|| ScopedFuture::new(get_repo("team/private".to_string())))
				.await
				.unwrap()
				.is_none()
		);
		membership.assert_calls_async(2).await;
	}
}

#[tokio::test]
async fn removed_organization_member_cannot_read_organization_repositories() {
	let server = MockServer::start_async().await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/team");
			then.status(404);
		})
		.await;
	let state = organization_state(&server).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let repositories = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap();
	assert_eq!(repositories.len(), 2);
	assert!(
		repositories
			.iter()
			.all(|repository| repository.installation_login == "alice")
	);
	membership.assert_calls_async(1).await;
}

#[tokio::test]
async fn expired_oauth_token_requires_sign_in_instead_of_exposing_organization_repositories() {
	let server = MockServer::start_async().await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/team");
			then.status(401);
		})
		.await;
	let state = organization_state(&server).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let error = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap_err();
	assert!(error.to_string().contains("sign in again"));
	membership.assert_calls_async(1).await;
}

#[tokio::test]
async fn provider_failure_is_a_repository_error_instead_of_an_empty_workspace() {
	for status in [403, 500] {
		let server = MockServer::start_async().await;
		let membership = server
			.mock_async(|when, then| {
				when.method(GET).path("/user/memberships/orgs/team");
				then.status(status);
			})
			.await;
		let state = organization_state(&server).await;
		let cookie = session(&state, 1);
		let owner = context(state, Some(cookie));
		let error = owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err();
		assert!(
			error
				.to_string()
				.contains("organization access could not be verified")
		);
		membership.assert_calls_async(1).await;
	}
}

#[tokio::test]
async fn malformed_membership_is_a_repository_error() {
	let server = MockServer::start_async().await;
	let membership = server
		.mock_async(|when, then| {
			when.method(GET).path("/user/memberships/orgs/team");
			then.json_body(serde_json::json!({"state":"active"}));
		})
		.await;
	let state = organization_state(&server).await;
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let error = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap_err();
	assert!(
		error
			.to_string()
			.contains("organization access could not be verified")
	);
	membership.assert_calls_async(1).await;
}

#[tokio::test]
async fn missing_oauth_token_requires_sign_in_without_a_provider_request() {
	let server = MockServer::start_async().await;
	let outbound = server
		.mock_async(|when, then| {
			when.any_request();
			then.status(500);
		})
		.await;
	let state = organization_state(&server).await;
	sqlx::query("UPDATE users SET github_access_token = '' WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	let error = owner
		.with(|| ScopedFuture::new(list_repos()))
		.await
		.unwrap_err();
	assert!(error.to_string().contains("sign in again"));
	outbound.assert_calls_async(0).await;
}

#[tokio::test]
async fn deleted_user_and_mismatched_identity_cannot_use_an_old_session() {
	let state = state().await;
	let token = create_token(&state.jwt_secret, 1, 202, "bob").unwrap();
	let owner = context(
		state.clone(),
		Some(format!("{}={token}", oauth::SESSION_COOKIE_NAME)),
	);
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err()
			.to_string()
			.contains("Invalid session")
	);
	let cookie = session(&state, 1);
	sqlx::query("DELETE FROM users WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();
	let owner = context(state, Some(cookie));
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err()
			.to_string()
			.contains("Invalid session")
	);
}

#[tokio::test]
async fn unknown_installation_account_type_cannot_grant_repository_access() {
	let state = state().await;
	sqlx::query("UPDATE installations SET github_account_type = 'Unknown' WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap()
			.is_empty()
	);
}

#[tokio::test]
async fn personal_repositories_do_not_need_a_provider_membership_request() {
	let server = MockServer::start_async().await;
	let outbound = server
		.mock_async(|when, then| {
			when.any_request();
			then.status(500);
		})
		.await;
	let state = state().await;
	sqlx::query("UPDATE users SET github_access_token = '' WHERE id = 1")
		.execute(&state.db)
		.await
		.unwrap();
	let mut state = (*state).clone();
	state.github_api_origin = server.base_url();
	let state = Arc::new(state);
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	assert_eq!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap()
			.len(),
		2
	);
	outbound.assert_calls_async(0).await;
}

#[tokio::test]
async fn invalid_provider_origin_is_an_explicit_repository_error() {
	let server = MockServer::start_async().await;
	let state = organization_state(&server).await;
	let mut state = (*state).clone();
	state.github_api_origin = "not a URL".to_string();
	let state = Arc::new(state);
	let cookie = session(&state, 1);
	let owner = context(state, Some(cookie));
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err()
			.to_string()
			.contains("organization access could not be verified")
	);
}

#[tokio::test]
async fn database_failure_cannot_be_reported_as_an_empty_workspace() {
	let missing_table = state().await;
	let cookie = session(&missing_table, 1);
	sqlx::query("DROP TABLE repositories")
		.execute(&missing_table.db)
		.await
		.unwrap();
	let owner = context(missing_table, Some(cookie));
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err()
			.to_string()
			.contains("DB:")
	);
	let closed_pool = state().await;
	let cookie = session(&closed_pool, 1);
	closed_pool.db.close().await;
	let owner = context(closed_pool, Some(cookie));
	assert!(
		owner
			.with(|| ScopedFuture::new(list_repos()))
			.await
			.unwrap_err()
			.to_string()
			.contains("DB:")
	);
}
