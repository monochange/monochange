//! The client-session store's single-use, expiry, and revocation behavior.

#![allow(clippy::disallowed_methods)]

use crate::DbPool;
use crate::client_sessions::*;

async fn pool() -> DbPool {
	let pool = crate::create_pool("sqlite::memory:")
		.await
		.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
	crate::run_migrations(&pool)
		.await
		.unwrap_or_else(|error| panic!("run sqlite migrations: {error}"));
	pool
}

async fn user(pool: &DbPool, id: i32, github_id: i64) {
	sqlx::query(
		"INSERT INTO users (id, github_id, github_login, github_access_token)
		 VALUES (?, ?, ?, '')",
	)
	.bind(id)
	.bind(github_id)
	.bind(format!("user-{id}"))
	.execute(pool)
	.await
	.unwrap();
}

#[tokio::test]
async fn migration_adds_epoch_grants_and_sessions() {
	let pool = pool().await;
	let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('users')")
		.fetch_all(&pool)
		.await
		.unwrap();
	assert!(columns.contains(&"session_epoch".to_string()));

	for table in ["client_grants", "client_sessions"] {
		let columns: Vec<String> =
			sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
				.fetch_all(&pool)
				.await
				.unwrap();
		assert!(!columns.is_empty(), "{table} should exist");
	}

	let indexes: Vec<String> = sqlx::query_scalar(
		"SELECT name FROM sqlite_master WHERE type = 'index' AND name LIKE 'idx_client_%'",
	)
	.fetch_all(&pool)
	.await
	.unwrap();
	assert!(indexes.contains(&"idx_client_grants_user".to_string()));
	assert!(indexes.contains(&"idx_client_grants_expiry".to_string()));
	assert!(indexes.contains(&"idx_client_sessions_user".to_string()));
	assert!(indexes.contains(&"idx_client_sessions_expiry".to_string()));
	assert!(indexes.contains(&"idx_client_sessions_token".to_string()));
}

#[tokio::test]
async fn grants_are_single_use() {
	let pool = pool().await;
	user(&pool, 1, 101).await;

	let code = issue_grant(
		&pool,
		1,
		"native",
		"dev.monochange.app:/auth/native/return",
		"challenge",
		0,
	)
	.await
	.unwrap();
	assert!(code.len() <= MAX_CLIENT_CODE_LEN);

	let consumed = consume_grant(&pool, &code).await.unwrap().unwrap();
	assert_eq!(consumed.user_id, 1);
	assert_eq!(consumed.client, "native");
	assert_eq!(
		consumed.redirect_uri,
		"dev.monochange.app:/auth/native/return"
	);
	assert_eq!(consumed.code_challenge, "challenge");

	assert!(consume_grant(&pool, &code).await.unwrap().is_none());
	assert!(
		consume_grant(&pool, "never-issued")
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn only_hashes_are_stored() {
	let pool = pool().await;
	user(&pool, 1, 101).await;

	let code = issue_grant(
		&pool,
		1,
		"native",
		"dev.monochange.app:/auth/native/return",
		"challenge",
		0,
	)
	.await
	.unwrap();
	let stored_code: String = sqlx::query_scalar("SELECT code_hash FROM client_grants")
		.fetch_one(&pool)
		.await
		.unwrap();
	assert_eq!(stored_code, hash_secret(&code));
	assert_ne!(stored_code, code);

	let session = create_session(&pool, 1, "native", 0, "mcd_secret")
		.await
		.unwrap();
	assert!(session.id.starts_with("mcs_"));
	assert_eq!(session.id.len(), "mcs_".len() + 32);
	let stored_token: String =
		sqlx::query_scalar("SELECT token_hash FROM client_sessions WHERE id = ?")
			.bind(&session.id)
			.fetch_one(&pool)
			.await
			.unwrap();
	assert_eq!(stored_token, hash_secret("mcd_secret"));
	assert_ne!(stored_token, "mcd_secret");
}

#[tokio::test]
async fn sessions_resolve_until_expiry_or_epoch_change() {
	let pool = pool().await;
	user(&pool, 1, 101).await;

	let session = create_session(&pool, 1, "native", 0, "mcd_secret")
		.await
		.unwrap();
	let resolved = resolve_session(&pool, "mcd_secret").await.unwrap().unwrap();
	assert_eq!(resolved.id, session.id);
	assert_eq!(resolved.user_id, 1);
	assert_eq!(resolved.github_id, 101);
	assert_eq!(resolved.client, "native");

	assert!(
		resolve_session(&pool, "unknown-token")
			.await
			.unwrap()
			.is_none()
	);

	sqlx::query("UPDATE users SET session_epoch = 1 WHERE id = 1")
		.execute(&pool)
		.await
		.unwrap();
	assert!(
		resolve_session(&pool, "mcd_secret")
			.await
			.unwrap()
			.is_none()
	);

	sqlx::query("UPDATE users SET session_epoch = 0 WHERE id = 1")
		.execute(&pool)
		.await
		.unwrap();
	assert!(
		resolve_session(&pool, "mcd_secret")
			.await
			.unwrap()
			.is_some()
	);

	sqlx::query("UPDATE client_sessions SET expires_at = ? WHERE id = ?")
		.bind(chrono::Utc::now().timestamp() - 1)
		.bind(&session.id)
		.execute(&pool)
		.await
		.unwrap();
	assert!(
		resolve_session(&pool, "mcd_secret")
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn resolving_touches_last_used_at_at_most_once_an_hour() {
	let pool = pool().await;
	user(&pool, 1, 101).await;

	let session = create_session(&pool, 1, "native", 0, "mcd_secret")
		.await
		.unwrap();
	// A fresh session is inside the touch interval, so a resolve leaves it.
	let before: i64 = sqlx::query_scalar("SELECT last_used_at FROM client_sessions WHERE id = ?")
		.bind(&session.id)
		.fetch_one(&pool)
		.await
		.unwrap();
	resolve_session(&pool, "mcd_secret").await.unwrap().unwrap();
	let after: i64 = sqlx::query_scalar("SELECT last_used_at FROM client_sessions WHERE id = ?")
		.bind(&session.id)
		.fetch_one(&pool)
		.await
		.unwrap();
	assert_eq!(
		after, before,
		"an in-interval resolve must not rewrite the row"
	);

	// An hour-old last use is refreshed on the next successful resolve.
	let now = chrono::Utc::now().timestamp();
	sqlx::query("UPDATE client_sessions SET last_used_at = ? WHERE id = ?")
		.bind(now - 7200)
		.bind(&session.id)
		.execute(&pool)
		.await
		.unwrap();
	resolve_session(&pool, "mcd_secret").await.unwrap().unwrap();
	let touched: i64 = sqlx::query_scalar("SELECT last_used_at FROM client_sessions WHERE id = ?")
		.bind(&session.id)
		.fetch_one(&pool)
		.await
		.unwrap();
	assert!(
		touched > now - 60,
		"the stale last-used timestamp must be refreshed: {touched} vs {now}"
	);
}

#[tokio::test]
async fn revoking_a_token_is_idempotent_and_bounded_to_the_owner() {
	let pool = pool().await;
	user(&pool, 1, 101).await;
	user(&pool, 2, 202).await;

	let first = create_session(&pool, 1, "native", 0, "mcd_first")
		.await
		.unwrap();
	let second = create_session(&pool, 2, "extension", 0, "mcd_second")
		.await
		.unwrap();

	assert!(revoke_session(&pool, "mcd_first").await.unwrap());
	assert!(!revoke_session(&pool, "mcd_first").await.unwrap());
	assert!(resolve_session(&pool, "mcd_first").await.unwrap().is_none());
	assert!(
		resolve_session(&pool, "mcd_second")
			.await
			.unwrap()
			.is_some()
	);

	assert!(!revoke_session_by_id(&pool, 2, &first.id).await.unwrap());
	assert!(revoke_session_by_id(&pool, 2, &second.id).await.unwrap());
	assert!(
		resolve_session(&pool, "mcd_second")
			.await
			.unwrap()
			.is_none()
	);
}

#[tokio::test]
async fn listing_returns_only_the_users_sessions_newest_first() {
	let pool = pool().await;
	user(&pool, 1, 101).await;
	user(&pool, 2, 202).await;

	let first = create_session(&pool, 1, "native", 0, "mcd_first")
		.await
		.unwrap();
	let second = create_session(&pool, 1, "extension", 0, "mcd_second")
		.await
		.unwrap();
	create_session(&pool, 2, "native", 0, "mcd_other")
		.await
		.unwrap();

	let listed = list_sessions(&pool, 1).await.unwrap();
	let ids: Vec<&str> = listed.iter().map(|session| session.id.as_str()).collect();
	assert_eq!(ids, vec![second.id.as_str(), first.id.as_str()]);
	assert_eq!(listed[0].client, "extension");
	assert_eq!(listed[0].last_used_at, Some(listed[0].created_at));
}

#[tokio::test]
async fn global_revocation_invalidates_grants_sessions_and_the_epoch() {
	let pool = pool().await;
	user(&pool, 1, 101).await;

	let code = issue_grant(
		&pool,
		1,
		"native",
		"dev.monochange.app:/auth/native/return",
		"challenge",
		0,
	)
	.await
	.unwrap();
	let session = create_session(&pool, 1, "native", 0, "mcd_secret")
		.await
		.unwrap();

	revoke_all_for_user(&pool, 1).await.unwrap();

	let epoch: i64 = sqlx::query_scalar("SELECT session_epoch FROM users WHERE id = 1")
		.fetch_one(&pool)
		.await
		.unwrap();
	assert_eq!(epoch, 1);
	assert!(consume_grant(&pool, &code).await.unwrap().is_none());
	assert!(
		resolve_session(&pool, "mcd_secret")
			.await
			.unwrap()
			.is_none()
	);
	assert!(list_sessions(&pool, 1).await.unwrap().is_empty());
	assert_eq!(session.user_id, 1);
}

#[tokio::test]
async fn expired_rows_are_pruned() {
	let pool = pool().await;
	user(&pool, 1, 101).await;

	let code = issue_grant(
		&pool,
		1,
		"native",
		"dev.monochange.app:/auth/native/return",
		"challenge",
		0,
	)
	.await
	.unwrap();
	let session = create_session(&pool, 1, "native", 0, "mcd_secret")
		.await
		.unwrap();
	let now = chrono::Utc::now().timestamp();
	sqlx::query("UPDATE client_grants SET expires_at = ?")
		.bind(now - 1)
		.execute(&pool)
		.await
		.unwrap();
	sqlx::query("UPDATE client_sessions SET expires_at = ?")
		.bind(now - 1)
		.execute(&pool)
		.await
		.unwrap();

	prune_expired(&pool).await.unwrap();

	assert!(consume_grant(&pool, &code).await.unwrap().is_none());
	assert!(
		resolve_session(&pool, "mcd_secret")
			.await
			.unwrap()
			.is_none()
	);
	assert_eq!(session.user_id, 1);
}

#[tokio::test]
async fn migration_rolls_back_and_reapplies() {
	use welds::migrations::MigrationWriter;

	let pool = pool().await;
	for statement in crate::CreateClientSessions.down_sql(welds::Syntax::Sqlite) {
		sqlx::query(&statement).execute(&pool).await.unwrap();
	}
	let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('users')")
		.fetch_all(&pool)
		.await
		.unwrap();
	assert!(!columns.contains(&"session_epoch".to_string()));

	for statement in crate::CreateClientSessions.up_sql(welds::Syntax::Sqlite) {
		sqlx::query(&statement).execute(&pool).await.unwrap();
	}
	user(&pool, 1, 101).await;
	assert!(
		issue_grant(
			&pool,
			1,
			"native",
			"dev.monochange.app:/auth/native/return",
			"challenge",
			0
		)
		.await
		.is_ok()
	);
}

#[test]
fn hash_secret_is_deterministic_and_not_the_input() {
	assert_eq!(hash_secret("same"), hash_secret("same"));
	assert_ne!(hash_secret("same"), hash_secret("other"));
	assert_ne!(hash_secret("mcd_secret"), "mcd_secret");
	assert_eq!(hash_secret("").len(), 64);
}
