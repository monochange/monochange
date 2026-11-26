//! Repository merge-velocity tracking and high-frequency burst detection.
//!
//! Tracks merged pull requests per connected repository so `monochange.dev` can
//! detect when a repository crosses the high-velocity threshold (more than 1 PR
//! every 10 minutes on average over at least an hour — i.e., 7 or more merged
//! PRs inside any 60-minute sliding window within the past 24 hours) and
//! suggest switching to frozen release cuts (`strategy = "cut"`).

use sqlx::Row;

use crate::DbPool;

/// Rolling lookback period inspected for high-velocity merge bursts (24 hours).
pub const VELOCITY_LOOKBACK_SECS: i64 = 24 * 60 * 60;

/// Sliding window length used to detect sustained merge velocity (1 hour).
pub const VELOCITY_WINDOW_SECS: i64 = 60 * 60;

/// Retention period for stored merge timestamps before old rows are pruned (48 hours).
pub const VELOCITY_RETENTION_SECS: i64 = 48 * 60 * 60;

/// Minimum merged PR count within a single 60-minute window to qualify as
/// strictly more than 1 PR every 10 minutes (`> 6` PRs per hour).
pub const HIGH_VELOCITY_MIN_PRS_PER_HOUR: usize = 7;

/// Detected high-velocity burst within the last 24 hours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VelocityBurst {
	/// Peak number of merged PRs observed within a 60-minute window (`>= 7`).
	pub peak_prs_in_hour: usize,
	/// Total number of merged PRs observed across the 24-hour lookback window.
	pub merged_prs_24h: usize,
	/// Earliest merge timestamp (Unix seconds) in the peak window.
	pub window_start_unix: i64,
	/// Latest merge timestamp (Unix seconds) in the peak window.
	pub window_end_unix: i64,
}

impl VelocityBurst {
	/// Format a concise human-readable cadence summary for the peak burst.
	#[must_use]
	pub fn format_summary(&self) -> String {
		let avg_minutes = 60.0 / (self.peak_prs_in_hour as f64);
		format!(
			"{} PRs merged in 1 hour (~1 every {:.1} min · {} in the last 24h)",
			self.peak_prs_in_hour, avg_minutes, self.merged_prs_24h
		)
	}
}

/// Inspect `merged_at_timestamps` (Unix seconds) within the 24-hour window ending
/// at `now_unix` and return the strongest 60-minute burst when any 60-minute
/// window contains at least [`HIGH_VELOCITY_MIN_PRS_PER_HOUR`] merges.
#[must_use]
pub fn detect_high_velocity_burst(
	merged_at_timestamps: &[i64],
	now_unix: i64,
) -> Option<VelocityBurst> {
	let cutoff = now_unix.saturating_sub(VELOCITY_LOOKBACK_SECS);
	let mut recent: Vec<i64> = merged_at_timestamps
		.iter()
		.copied()
		.filter(|&timestamp| timestamp >= cutoff && timestamp <= now_unix)
		.collect();

	if recent.len() < HIGH_VELOCITY_MIN_PRS_PER_HOUR {
		return None;
	}

	recent.sort_unstable();

	let merged_prs_24h = recent.len();
	let mut best_count = 0_usize;
	let mut best_start = 0_i64;
	let mut best_end = 0_i64;
	let mut left = 0_usize;

	for (right, &right_ts) in recent.iter().enumerate() {
		while let Some(&left_ts) = recent.get(left)
			&& right_ts.saturating_sub(left_ts) > VELOCITY_WINDOW_SECS
		{
			left += 1;
		}

		let count = right - left + 1;
		if count > best_count
			&& let Some(&left_ts) = recent.get(left)
		{
			best_count = count;
			best_start = left_ts;
			best_end = right_ts;
		}
	}

	(best_count >= HIGH_VELOCITY_MIN_PRS_PER_HOUR).then_some(VelocityBurst {
		peak_prs_in_hour: best_count,
		merged_prs_24h,
		window_start_unix: best_start,
		window_end_unix: best_end,
	})
}

/// Record a merged pull request timestamp for `github_repo_id` and prune rows
/// older than [`VELOCITY_RETENTION_SECS`].
pub async fn record_merged_pull_request(
	pool: &DbPool,
	github_repo_id: i64,
	pull_request_number: i64,
	base_branch: &str,
	merged_at_unix: i64,
) -> Result<(), sqlx::Error> {
	sqlx::query(
		"INSERT OR IGNORE INTO repository_merged_prs (github_repo_id, pull_request_number, base_branch, merged_at)
		 VALUES ($1, $2, $3, $4)",
	)
	.bind(github_repo_id)
	.bind(pull_request_number)
	.bind(base_branch)
	.bind(merged_at_unix)
	.execute(pool)
	.await?;

	let retention_cutoff = merged_at_unix.saturating_sub(VELOCITY_RETENTION_SECS);
	sqlx::query(
		"DELETE FROM repository_merged_prs
		 WHERE github_repo_id = $1 AND merged_at < $2",
	)
	.bind(github_repo_id)
	.bind(retention_cutoff)
	.execute(pool)
	.await?;

	Ok(())
}

/// Load merged pull request timestamps for `github_repo_id` at or after `since_unix`.
pub async fn recent_merged_timestamps(
	pool: &DbPool,
	github_repo_id: i64,
	since_unix: i64,
) -> Result<Vec<i64>, sqlx::Error> {
	let rows = sqlx::query(
		"SELECT merged_at
		 FROM repository_merged_prs
		 WHERE github_repo_id = $1 AND merged_at >= $2
		 ORDER BY merged_at ASC",
	)
	.bind(github_repo_id)
	.bind(since_unix)
	.fetch_all(pool)
	.await?;

	Ok(rows.into_iter().map(|row| row.get("merged_at")).collect())
}

#[cfg(test)]
#[path = "__tests__/velocity_tests.rs"]
mod tests;
