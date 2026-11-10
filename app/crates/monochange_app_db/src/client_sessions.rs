//! Opaque client-session grants and tokens.
//!
//! Only hashes of one-time codes and bearer tokens are stored. A grant is
//! consumed by an atomic `DELETE ... RETURNING`, so a code cannot be replayed
//! even when the verifier, client, or redirect is wrong. Every row is bound to
//! the user's `session_epoch`, so incrementing that epoch revokes all of a
//! user's grants and client sessions in one transaction.

use chrono::Utc;
use sha2::Digest;
use sqlx::Row;

use crate::DbPool;

/// How long a one-time approval code stays exchangeable.
pub const CLIENT_GRANT_TTL_SECS: i64 = 120;
/// How long an opaque client token stays valid.
pub const CLIENT_TOKEN_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// How often a client session's `last_used_at` may be touched.
pub const CLIENT_SESSION_TOUCH_INTERVAL_SECS: i64 = 60 * 60;
/// The longest one-time code accepted from a client.
pub const MAX_CLIENT_CODE_LEN: usize = 256;
/// The longest opaque client token accepted from a client.
pub const MAX_CLIENT_TOKEN_LEN: usize = 512;

/// The SHA-256 hash stored in place of a code or token.
#[must_use]
pub fn hash_secret(value: &str) -> String {
	hex::encode(sha2::Sha256::digest(value.as_bytes()))
}

/// A consumed approval code and the values it was bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRecord {
	pub user_id: i32,
	pub client: String,
	pub redirect_uri: String,
	pub code_challenge: String,
	pub session_epoch: i64,
	pub expires_at: i64,
}

/// A stored client session, without its bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSessionRow {
	pub id: String,
	pub user_id: i32,
	pub client: String,
	pub session_epoch: i64,
	pub created_at: i64,
	pub last_used_at: Option<i64>,
	pub expires_at: i64,
}

/// A client token that resolved to a user, with the epoch that authorized it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedClientSession {
	pub id: String,
	pub user_id: i32,
	pub github_id: i64,
	pub github_login: String,
	pub github_avatar_url: Option<String>,
	pub plan_tier: String,
	pub client: String,
	pub session_epoch: i64,
	pub expires_at: i64,
}

/// Store a new one-time approval code and return the opaque code to redirect.
///
/// # Errors
/// Returns database failures so a caller cannot treat a failed insert as an
/// issued code.
pub async fn issue_grant(
	pool: &DbPool,
	user_id: i32,
	client: &str,
	redirect_uri: &str,
	code_challenge: &str,
	session_epoch: i64,
) -> Result<String, sqlx::Error> {
	let code = format!("mcc_{}", uuid::Uuid::new_v4().simple());
	let code_hash = hash_secret(&code);
	let expires_at = Utc::now().timestamp() + CLIENT_GRANT_TTL_SECS;

	prune_expired(pool).await?;

	sqlx::query(
		"INSERT INTO client_grants (
			code_hash, user_id, client, redirect_uri, code_challenge, session_epoch, expires_at
		 ) VALUES (?, ?, ?, ?, ?, ?, ?)",
	)
	.bind(&code_hash)
	.bind(user_id)
	.bind(client)
	.bind(redirect_uri)
	.bind(code_challenge)
	.bind(session_epoch)
	.bind(expires_at)
	.execute(pool)
	.await?;

	Ok(code)
}

/// Atomically consume a one-time code, returning the grant it named.
///
/// The delete happens before any validation, so a wrong verifier, client, or
/// redirect burns the code instead of leaving it reusable.
///
/// # Errors
/// Returns database failures; a missing code yields `Ok(None)`.
pub async fn consume_grant(pool: &DbPool, code: &str) -> Result<Option<GrantRecord>, sqlx::Error> {
	let code_hash = hash_secret(code);
	let row = sqlx::query(
		"DELETE FROM client_grants WHERE code_hash = ?
		 RETURNING user_id, client, redirect_uri, code_challenge, session_epoch, expires_at",
	)
	.bind(&code_hash)
	.fetch_optional(pool)
	.await?;

	let Some(row) = row else {
		return Ok(None);
	};

	Ok(Some(GrantRecord {
		user_id: row.get("user_id"),
		client: row.get("client"),
		redirect_uri: row.get("redirect_uri"),
		code_challenge: row.get("code_challenge"),
		session_epoch: row.get("session_epoch"),
		expires_at: row.get("expires_at"),
	}))
}

/// Store a new opaque client session and return its public row.
///
/// # Errors
/// Returns database failures so a caller cannot treat a failed insert as a
/// stored session.
pub async fn create_session(
	pool: &DbPool,
	user_id: i32,
	client: &str,
	session_epoch: i64,
	access_token: &str,
) -> Result<ClientSessionRow, sqlx::Error> {
	let id = format!("mcs_{}", uuid::Uuid::new_v4().simple());
	let token_hash = hash_secret(access_token);
	let now = Utc::now().timestamp();
	let expires_at = now + CLIENT_TOKEN_TTL_SECS;

	sqlx::query(
		"INSERT INTO client_sessions (
			id, user_id, token_hash, client, session_epoch, created_at, last_used_at, expires_at
		 ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
	)
	.bind(&id)
	.bind(user_id)
	.bind(&token_hash)
	.bind(client)
	.bind(session_epoch)
	.bind(now)
	.bind(now)
	.bind(expires_at)
	.execute(pool)
	.await?;

	Ok(ClientSessionRow {
		id,
		user_id,
		client: client.to_owned(),
		session_epoch,
		created_at: now,
		last_used_at: Some(now),
		expires_at,
	})
}

/// Resolve an opaque client token to its user, touching `last_used_at` at most
/// once an hour. Expired, revoked, and epoch-mismatched tokens resolve to
/// `None`.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as a missing
/// session.
pub async fn resolve_session(
	pool: &DbPool,
	access_token: &str,
) -> Result<Option<ResolvedClientSession>, sqlx::Error> {
	let token_hash = hash_secret(access_token);
	let row = sqlx::query(
		"SELECT s.id, s.user_id, s.client, s.session_epoch, s.expires_at, s.last_used_at,
		        u.github_id, u.github_login, u.github_avatar_url, u.plan_tier,
		        u.session_epoch AS user_epoch
		 FROM client_sessions s
		 JOIN users u ON u.id = s.user_id
		 WHERE s.token_hash = ?",
	)
	.bind(&token_hash)
	.fetch_optional(pool)
	.await?;

	let Some(row) = row else {
		return Ok(None);
	};

	let session_epoch: i64 = row.get("session_epoch");
	let user_epoch: i64 = row.get("user_epoch");
	let expires_at: i64 = row.get("expires_at");
	let now = Utc::now().timestamp();

	if session_epoch != user_epoch || expires_at <= now {
		return Ok(None);
	}

	let last_used_at: Option<i64> = row.get("last_used_at");
	if last_used_at.is_none_or(|used| now - used >= CLIENT_SESSION_TOUCH_INTERVAL_SECS) {
		sqlx::query("UPDATE client_sessions SET last_used_at = ? WHERE id = ?")
			.bind(now)
			.bind(row.get::<String, _>("id"))
			.execute(pool)
			.await?;
	}

	Ok(Some(ResolvedClientSession {
		id: row.get("id"),
		user_id: row.get("user_id"),
		github_id: row.get("github_id"),
		github_login: row.get("github_login"),
		github_avatar_url: row.get("github_avatar_url"),
		plan_tier: row.get("plan_tier"),
		client: row.get("client"),
		session_epoch,
		expires_at,
	}))
}

/// Revoke the session named by an opaque token. Returns whether a row matched.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as a revoked
/// session.
pub async fn revoke_session(pool: &DbPool, access_token: &str) -> Result<bool, sqlx::Error> {
	let token_hash = hash_secret(access_token);
	let deleted = sqlx::query("DELETE FROM client_sessions WHERE token_hash = ?")
		.bind(&token_hash)
		.execute(pool)
		.await?;
	Ok(deleted.rows_affected() > 0)
}

/// Revoke one of a user's own sessions by id. Returns whether a row matched.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as a revoked
/// session.
pub async fn revoke_session_by_id(
	pool: &DbPool,
	user_id: i32,
	id: &str,
) -> Result<bool, sqlx::Error> {
	let deleted = sqlx::query("DELETE FROM client_sessions WHERE id = ? AND user_id = ?")
		.bind(id)
		.bind(user_id)
		.execute(pool)
		.await?;
	Ok(deleted.rows_affected() > 0)
}

/// List a user's client sessions, newest first.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as an empty
/// list.
pub async fn list_sessions(
	pool: &DbPool,
	user_id: i32,
) -> Result<Vec<ClientSessionRow>, sqlx::Error> {
	let rows = sqlx::query(
		"SELECT id, user_id, client, session_epoch, created_at, last_used_at, expires_at
		 FROM client_sessions WHERE user_id = ? ORDER BY created_at DESC, rowid DESC",
	)
	.bind(user_id)
	.fetch_all(pool)
	.await?;

	Ok(rows
		.into_iter()
		.map(|row| {
			ClientSessionRow {
				id: row.get("id"),
				user_id: row.get("user_id"),
				client: row.get("client"),
				session_epoch: row.get("session_epoch"),
				created_at: row.get("created_at"),
				last_used_at: row.get("last_used_at"),
				expires_at: row.get("expires_at"),
			}
		})
		.collect())
}

/// Increment a user's epoch and delete every grant and client session they
/// own, in one transaction. Old browser JWTs and client tokens stop resolving
/// immediately.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as a
/// completed revocation.
pub async fn revoke_all_for_user(pool: &DbPool, user_id: i32) -> Result<(), sqlx::Error> {
	let mut transaction = pool.begin().await?;

	sqlx::query(
		"UPDATE users
		 SET session_epoch = session_epoch + 1,
		     updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
		 WHERE id = ?",
	)
	.bind(user_id)
	.execute(&mut *transaction)
	.await?;

	sqlx::query("DELETE FROM client_grants WHERE user_id = ?")
		.bind(user_id)
		.execute(&mut *transaction)
		.await?;

	sqlx::query("DELETE FROM client_sessions WHERE user_id = ?")
		.bind(user_id)
		.execute(&mut *transaction)
		.await?;

	transaction.commit().await?;
	Ok(())
}

/// Delete grants and client sessions that have passed their expiry.
///
/// # Errors
/// Returns database failures so a caller cannot treat an outage as a
/// completed cleanup.
pub async fn prune_expired(pool: &DbPool) -> Result<(), sqlx::Error> {
	let now = Utc::now().timestamp();

	sqlx::query("DELETE FROM client_grants WHERE expires_at <= ?")
		.bind(now)
		.execute(pool)
		.await?;

	sqlx::query("DELETE FROM client_sessions WHERE expires_at <= ?")
		.bind(now)
		.execute(pool)
		.await?;

	Ok(())
}

#[cfg(test)]
#[path = "__tests__/client_sessions_tests.rs"]
mod tests;
