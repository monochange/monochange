//! Installation webhooks must persist before the installer completes website sign-in.

#![allow(clippy::disallowed_methods)]

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use hmac::Hmac;
use hmac::Mac;
use sha2::Sha256;
use tower::ServiceExt;

use crate::AppSecrets;
use crate::AppState;
use crate::api_router;
use crate::github_app::GitHubAppAuth;

#[tokio::test]
async fn first_installation_is_saved_before_the_installer_has_a_website_account() {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({})).unwrap();
	let mut state = AppState::new(db.clone(), secrets).unwrap();
	state.github_app = Some(GitHubAppAuth::new(
		"123",
		"",
		"test-only-webhook-secret",
		"https://api.github.com",
	));
	let payload = serde_json::json!({
		"action": "created",
		"installation": {"id":1001, "account":{"id":101,"login":"alice","type":"User"}, "repository_selection":"all"},
		"sender": {"id":101,"login":"alice","avatar_url":null},
		"repositories": [{"id":11,"full_name":"alice/project","private":true}]
	}).to_string();
	let mut mac = Hmac::<Sha256>::new_from_slice(b"test-only-webhook-secret").unwrap();
	mac.update(payload.as_bytes());
	let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
	let response = api_router(state)
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/api/github/webhooks")
				.header("x-hub-signature-256", signature)
				.header("x-github-event", "installation")
				.body(Body::from(payload))
				.unwrap(),
		)
		.await
		.unwrap();
	assert_eq!(
		response.status(),
		StatusCode::OK,
		"a valid installation must not fail its users foreign key"
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
			.fetch_one(&db)
			.await
			.unwrap(),
		1
	);
}

async fn configured_state() -> AppState {
	let db = monochange_app_db::create_pool("sqlite::memory:")
		.await
		.unwrap();
	monochange_app_db::run_migrations(&db).await.unwrap();
	let secrets: AppSecrets = serde_json::from_value(serde_json::json!({})).unwrap();
	let mut state = AppState::new(db, secrets).unwrap();
	state.github_app = Some(GitHubAppAuth::new(
		"123",
		"",
		"test-only-webhook-secret",
		"https://api.github.com",
	));
	state
}

async fn deliver(state: &AppState, payload: serde_json::Value) -> StatusCode {
	let event = if matches!(payload["action"].as_str(), Some("added" | "removed")) {
		"installation_repositories"
	} else {
		"installation"
	};
	deliver_event(state, event, payload).await
}

async fn deliver_event(state: &AppState, event: &str, payload: serde_json::Value) -> StatusCode {
	let payload = payload.to_string();
	let mut mac = Hmac::<Sha256>::new_from_slice(b"test-only-webhook-secret").unwrap();
	mac.update(payload.as_bytes());
	let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
	api_router(state.clone())
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
		.unwrap()
		.status()
}

#[tokio::test]
async fn unrelated_signed_events_cannot_delete_or_create_installations() {
	let state = configured_state().await;
	let owner = installation(1001, "alice", 101);
	assert_eq!(deliver(&state, serde_json::json!({"action":"created","installation":owner,"repositories":[{"id":11,"full_name":"alice/project","private":true}]})).await, StatusCode::OK);
	assert_eq!(
		deliver_event(
			&state,
			"repository",
			serde_json::json!({"action":"deleted","installation":owner})
		)
		.await,
		StatusCode::OK
	);
	assert_eq!(deliver_event(&state, "issues", serde_json::json!({"action":"created","installation":installation(1002,"bob",202),"repositories":[]})).await, StatusCode::OK);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM installations")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		1
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		1
	);
}

async fn installation_tables(state: &AppState) -> [String; 3] {
	let users = sqlx::query_scalar("SELECT json_group_array(json_array(id, github_id, github_login, github_avatar_url, github_access_token, email, plan_tier, created_at, updated_at)) FROM (SELECT * FROM users ORDER BY id)")
		.fetch_one(&state.db)
		.await
		.unwrap();
	let installations = sqlx::query_scalar("SELECT json_group_array(json_array(id, user_id, github_installation_id, github_account_login, github_account_type, target_type, created_at, updated_at)) FROM (SELECT * FROM installations ORDER BY id)")
		.fetch_one(&state.db)
		.await
		.unwrap();
	let repositories = sqlx::query_scalar("SELECT json_group_array(json_array(id, installation_id, github_repo_id, github_full_name, github_private, monochange_config_hash, settings_json, plan_tier, created_at, updated_at)) FROM (SELECT * FROM repositories ORDER BY id)")
		.fetch_one(&state.db)
		.await
		.unwrap();
	[users, installations, repositories]
}

#[tokio::test]
async fn signed_repository_events_cannot_delete_the_parent_installation() {
	let state = configured_state().await;
	let owner = installation(1001, "alice", 101);
	let created = serde_json::json!({
		"action":"created", "installation":owner,
		"repositories":[{"id":11,"full_name":"alice/project","private":true}]
	});
	assert_eq!(deliver(&state, created).await, StatusCode::OK);
	let before = installation_tables(&state).await;
	let deleted = serde_json::json!({"action":"deleted","installation":owner});
	assert_eq!(
		deliver_event(&state, "installation_repositories", deleted.clone()).await,
		StatusCode::OK
	);
	assert_eq!(installation_tables(&state).await, before);

	assert_eq!(
		deliver_event(&state, "installation", deleted).await,
		StatusCode::OK
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM installations")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		0
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		0
	);
}

#[tokio::test]
async fn signed_installation_events_cannot_add_repositories() {
	let state = configured_state().await;
	let owner = installation(1001, "alice", 101);
	let created = serde_json::json!({
		"action":"created", "installation":owner,
		"repositories":[{"id":11,"full_name":"alice/project","private":true}]
	});
	assert_eq!(deliver(&state, created).await, StatusCode::OK);
	let before = installation_tables(&state).await;
	let added = serde_json::json!({
		"action":"added", "installation":owner,
		"repositories_added":[{"id":12,"full_name":"alice/second","private":false}]
	});
	assert_eq!(
		deliver_event(&state, "installation", added.clone()).await,
		StatusCode::OK
	);
	assert_eq!(installation_tables(&state).await, before);

	assert_eq!(
		deliver_event(&state, "installation_repositories", added).await,
		StatusCode::OK
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		2
	);
}

fn installation(id: i64, owner: &str, owner_id: i64) -> serde_json::Value {
	serde_json::json!({"id":id,"account":{"id":owner_id,"login":owner,"type":"User"}})
}

#[tokio::test]
async fn existing_oauth_identity_keeps_its_token_and_receives_the_installation() {
	let state = configured_state().await;
	sqlx::query("INSERT INTO users (id, github_id, github_login, github_access_token) VALUES (7, 101, 'alice', 'existing-test-token')").execute(&state.db).await.unwrap();
	let event = serde_json::json!({"action":"created","installation":installation(1001,"alice",101),"repositories":[{"id":11,"full_name":"alice/private","private":true}]});
	assert_eq!(deliver(&state, event.clone()).await, StatusCode::OK);
	assert_eq!(deliver(&state, event).await, StatusCode::OK);
	assert_eq!(
		sqlx::query_scalar::<_, i32>("SELECT user_id FROM installations")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		7
	);
	assert_eq!(
		sqlx::query_scalar::<_, String>("SELECT github_access_token FROM users WHERE id = 7")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		"existing-test-token"
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		1
	);
}

#[tokio::test]
async fn organization_installation_is_connected_to_the_verified_installer() {
	let state = configured_state().await;
	let event = serde_json::json!({"action":"created","installation":{"id":1001,"account":{"id":900,"login":"team","type":"Organization"}},"sender":{"id":101,"login":"alice"},"repositories":[{"id":11,"full_name":"team/project","private":true}]});
	assert_eq!(deliver(&state, event).await, StatusCode::OK);
	assert_eq!(
		sqlx::query_scalar::<_, i64>(
			"SELECT u.github_id FROM users u JOIN installations i ON i.user_id = u.id"
		)
		.fetch_one(&state.db)
		.await
		.unwrap(),
		101
	);
}

#[tokio::test]
async fn missing_owner_is_rejected_without_partial_installation_state() {
	let state = configured_state().await;
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"created","installation":{"id":1001},"repositories":[]})
		)
		.await,
		StatusCode::BAD_REQUEST
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM installations")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		0
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		0
	);
}

#[tokio::test]
async fn unconfigured_app_returns_a_failure_instead_of_dropping_webhooks() {
	let mut state = configured_state().await;
	state.github_app = None;
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"created","installation":installation(1001,"alice",101)})
		)
		.await,
		StatusCode::SERVICE_UNAVAILABLE
	);
}

#[tokio::test]
async fn repository_lifecycle_is_scoped_to_its_installation() {
	let state = configured_state().await;
	let alice = installation(1001, "alice", 101);
	let bob = installation(1002, "bob", 202);
	assert_eq!(deliver(&state, serde_json::json!({"action":"created","installation":alice,"repositories":[{"id":11,"full_name":"alice/project","private":true}]})).await, StatusCode::OK);
	assert_eq!(deliver(&state, serde_json::json!({"action":"created","installation":bob,"repositories":[{"id":21,"full_name":"bob/project","private":true}]})).await, StatusCode::OK);
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"removed","installation":alice,"repositories_removed":[{"id":21}]})
		)
		.await,
		StatusCode::OK
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories WHERE github_repo_id = 21")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		1
	);
	assert_eq!(deliver(&state, serde_json::json!({"action":"added","installation":alice,"repositories_added":[{"id":12,"full_name":"alice/second","private":false}]})).await, StatusCode::OK);
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"suspend","installation":alice})
		)
		.await,
		StatusCode::OK
	);
	assert_eq!(
		sqlx::query_scalar::<_, String>(
			"SELECT target_type FROM installations WHERE github_installation_id = 1001"
		)
		.fetch_one(&state.db)
		.await
		.unwrap(),
		"suspended"
	);
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"unsuspend","installation":alice})
		)
		.await,
		StatusCode::OK
	);
	assert_eq!(
		sqlx::query_scalar::<_, String>(
			"SELECT target_type FROM installations WHERE github_installation_id = 1001"
		)
		.fetch_one(&state.db)
		.await
		.unwrap(),
		"selected"
	);
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"removed","installation":alice,"repositories_removed":[{"id":12}]})
		)
		.await,
		StatusCode::OK
	);
	assert_eq!(
		deliver(
			&state,
			serde_json::json!({"action":"deleted","installation":alice})
		)
		.await,
		StatusCode::OK
	);
	assert_eq!(
		sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM repositories")
			.fetch_one(&state.db)
			.await
			.unwrap(),
		1
	);
}

#[tokio::test]
async fn installations_record_their_organisation_for_projects() {
	let state = configured_state().await;
	let status = deliver(
		&state,
		serde_json::json!({
			"action": "created",
			"installation": {"id": 2001, "account": {"id": 5001, "login": "acme", "type": "Organization", "avatar_url": "https://avatars/acme.png"}},
			"sender": {"id": 101, "login": "alice"},
			"repositories": [{"id": 31, "full_name": "acme/api", "private": false}]
		}),
	)
	.await;
	assert_eq!(status, StatusCode::OK);
	let (organization_id, login, account_type): (i32, String, String) = sqlx::query_as(
		"SELECT o.id, o.github_login, o.account_type FROM organizations o JOIN installations i ON i.organization_id = o.id WHERE i.github_installation_id = 2001",
	)
	.fetch_one(&state.db)
	.await
	.unwrap();
	assert_eq!(login, "acme");
	assert_eq!(account_type, "Organization");

	// A later event for the renamed account updates the same organisation.
	let status = deliver_event(
		&state,
		"installation_repositories",
		serde_json::json!({
			"action": "added",
			"installation": {"id": 2001, "account": {"id": 5001, "login": "acme-co", "type": "Organization"}},
			"repositories_added": [{"id": 32, "full_name": "acme-co/web", "private": true}]
		}),
	)
	.await;
	assert_eq!(status, StatusCode::OK);
	let renamed: (i32, String, Option<String>) =
		sqlx::query_as("SELECT id, github_login, github_avatar_url FROM organizations")
			.fetch_one(&state.db)
			.await
			.unwrap();
	assert_eq!(renamed.0, organization_id);
	assert_eq!(renamed.1, "acme-co");
	assert_eq!(renamed.2.as_deref(), Some("https://avatars/acme.png"));
}

#[tokio::test]
async fn installations_without_an_account_id_are_not_linked() {
	let state = configured_state().await;
	let status = deliver(
		&state,
		serde_json::json!({
			"action": "created",
			"installation": {"id": 2002, "account": {"login": "solo-org", "type": "Organization"}},
			"sender": {"id": 102, "login": "solo"},
			"repositories": []
		}),
	)
	.await;
	assert_eq!(status, StatusCode::OK);
	let linked: Option<i32> = sqlx::query_scalar(
		"SELECT organization_id FROM installations WHERE github_installation_id = 2002",
	)
	.fetch_one(&state.db)
	.await
	.unwrap();
	assert_eq!(linked, None);
}
