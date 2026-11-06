//! One-time consumption of verified browser OAuth state.

use crate::DbPool;

/// Claim a verified nonce before contacting the OAuth provider.
///
/// The unique insert selects one winner across requests and processes. Entries
/// remain until the signed state expires, including when provider exchange fails.
/// `SQLite` checks expiry during the insert because waiting for a connection or
/// writer can outlive the cookie and let another callback prune its used nonce.
///
/// # Errors
/// Returns database failures so callers cannot exchange a code without a replay guard.
pub async fn consume_state(
	pool: &DbPool,
	nonce: &str,
	expires_at: i64,
) -> Result<bool, sqlx::Error> {
	sqlx::query("DELETE FROM oauth_state_uses WHERE expires_at <= unixepoch()")
		.execute(pool)
		.await?;
	let inserted = sqlx::query(
		"INSERT INTO oauth_state_uses (nonce, expires_at)
		 SELECT $1, $2 WHERE $2 > unixepoch() ON CONFLICT(nonce) DO NOTHING",
	)
	.bind(nonce)
	.bind(expires_at)
	.execute(pool)
	.await?;

	Ok(inserted.rows_affected() == 1)
}

#[cfg(test)]
#[path = "__tests__/oauth_tests.rs"]
mod tests;
