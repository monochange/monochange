//! The replay guard selects exactly one callback and expires old entries.

#![allow(clippy::disallowed_methods)]

use super::consume_state;

async fn pool() -> crate::DbPool {
	let pool = crate::create_pool("sqlite::memory:").await.unwrap();
	crate::run_migrations(&pool).await.unwrap();
	pool
}

#[tokio::test]
async fn a_nonce_is_consumed_once_until_it_expires() {
	let pool = pool().await;
	let expiry = chrono::Utc::now().timestamp() + 600;

	assert!(
		consume_state(&pool, "one-time-nonce", expiry)
			.await
			.unwrap()
	);
	assert!(
		!consume_state(&pool, "one-time-nonce", expiry)
			.await
			.unwrap()
	);
	assert!(consume_state(&pool, "another-nonce", expiry).await.unwrap());
}

#[tokio::test]
async fn concurrent_claims_have_one_winner() {
	let pool = pool().await;
	let expiry = chrono::Utc::now().timestamp() + 600;
	let (first, second) = tokio::join!(
		consume_state(&pool, "shared-nonce", expiry),
		consume_state(&pool, "shared-nonce", expiry),
	);

	assert_ne!(first.unwrap(), second.unwrap());
}

#[tokio::test]
async fn a_queued_replay_cannot_reclaim_a_pruned_expired_nonce() {
	let pool = sqlx::sqlite::SqlitePoolOptions::new()
		.max_connections(1)
		.connect("sqlite::memory:")
		.await
		.unwrap();
	crate::run_migrations(&pool).await.unwrap();
	let expiry = chrono::Utc::now().timestamp() + 2;
	assert!(consume_state(&pool, "queued-replay", expiry).await.unwrap());
	let mut connection = pool.acquire().await.unwrap();
	let replay = consume_state(&pool, "queued-replay", expiry);
	tokio::pin!(replay);

	// Poll before expiry while pool acquisition is blocked by another request.
	assert!(
		tokio::time::timeout(std::time::Duration::from_millis(10), &mut replay)
			.await
			.is_err()
	);
	assert!(chrono::Utc::now().timestamp() < expiry);
	tokio::time::sleep(std::time::Duration::from_secs(2)).await;
	let expired: bool = sqlx::query_scalar("SELECT unixepoch() >= $1")
		.bind(expiry)
		.fetch_one(&mut *connection)
		.await
		.unwrap();
	assert!(expired);

	// A different callback prunes the used nonce before the queued replay resumes.
	let pruned = sqlx::query("DELETE FROM oauth_state_uses WHERE expires_at <= unixepoch()")
		.execute(&mut *connection)
		.await
		.unwrap();
	assert_eq!(pruned.rows_affected(), 1);
	drop(connection);

	assert!(!replay.await.unwrap());
}

#[tokio::test]
async fn expired_states_are_refused_and_old_rows_are_pruned() {
	let pool = pool().await;
	let now = chrono::Utc::now().timestamp();
	sqlx::query("INSERT INTO oauth_state_uses (nonce, expires_at) VALUES ('old', $1)")
		.bind(now - 1)
		.execute(&pool)
		.await
		.unwrap();

	assert!(!consume_state(&pool, "expired", now - 1).await.unwrap());
	let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM oauth_state_uses")
		.fetch_one(&pool)
		.await
		.unwrap();
	assert_eq!(count, 0);
}

#[tokio::test]
async fn storage_failure_cannot_claim_a_state() {
	let pool = pool().await;
	pool.close().await;

	assert!(
		consume_state(&pool, "unused", chrono::Utc::now().timestamp() + 600)
			.await
			.is_err()
	);
}

#[tokio::test]
async fn migration_can_be_rolled_back_and_reapplied() {
	use welds::migrations::MigrationWriter;

	let pool = pool().await;
	let writer = crate::CreateOAuthStateUses;

	for statement in writer.down_sql(welds::Syntax::Sqlite) {
		sqlx::query(&statement).execute(&pool).await.unwrap();
	}

	assert!(
		consume_state(&pool, "unused", chrono::Utc::now().timestamp() + 600)
			.await
			.is_err()
	);

	for statement in writer.up_sql(welds::Syntax::Sqlite) {
		sqlx::query(&statement).execute(&pool).await.unwrap();
	}

	assert!(
		consume_state(&pool, "unused", chrono::Utc::now().timestamp() + 600)
			.await
			.unwrap()
	);
}
