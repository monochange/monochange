#![allow(clippy::disallowed_methods)]

use super::*;

#[test]
fn detect_high_velocity_burst_requires_strictly_more_than_one_pr_per_ten_minutes_for_an_hour() {
	let now = 1_700_100_000_i64;
	// Exactly 6 PRs in 60 minutes (1 PR every 10 minutes) does not cross the threshold.
	let six_in_hour: Vec<i64> = (0..6).map(|index| now - (5 - index) * 600).collect();
	assert_eq!(detect_high_velocity_burst(&six_in_hour, now), None);

	// 7 PRs inside 60 minutes (~1 every 8.6 min) triggers the suggestion.
	let seven_in_hour: Vec<i64> = (0..7).map(|index| now - (6 - index) * 500).collect();
	let burst = detect_high_velocity_burst(&seven_in_hour, now)
		.unwrap_or_else(|| panic!("expected high-velocity burst"));
	assert_eq!(burst.peak_prs_in_hour, 7);
	assert_eq!(burst.merged_prs_24h, 7);
	assert!(burst.format_summary().contains("7 PRs merged in 1 hour"));
	assert!(burst.format_summary().contains("8.6 min"));
}

#[test]
fn detect_high_velocity_burst_ignores_spread_out_or_stale_merges_and_finds_peak_window_in_24h() {
	let now = 1_700_100_000_i64;
	// 7 PRs spaced 11 minutes apart span 66 minutes -> max 6 in any 60-minute slice.
	let spaced: Vec<i64> = (0..7).map(|index| now - (6 - index) * 660).collect();
	assert_eq!(detect_high_velocity_burst(&spaced, now), None);

	// 8 PRs in 1 hour from 26 hours ago (outside 24h lookback) -> ignored.
	let stale_base = now - 26 * 3600;
	let stale: Vec<i64> = (0..8).map(|index| stale_base + index * 300).collect();
	assert_eq!(detect_high_velocity_burst(&stale, now), None);

	// 8 PRs in 1 hour from 20 hours ago plus 2 scattered merges later in the day -> detected.
	let burst_base = now - 20 * 3600;
	let mut mixed: Vec<i64> = (0..8).map(|index| burst_base + index * 420).collect();
	mixed.push(now - 3600);
	mixed.push(now - 60);
	let burst = detect_high_velocity_burst(&mixed, now)
		.unwrap_or_else(|| panic!("expected burst from 20 hours ago"));
	assert_eq!(burst.peak_prs_in_hour, 8);
	assert_eq!(burst.merged_prs_24h, 10);
	assert_eq!(burst.window_start_unix, burst_base);
	assert_eq!(burst.window_end_unix, burst_base + 7 * 420);
}

#[tokio::test]
async fn record_merged_pull_request_deduplicates_queries_and_prunes_old_rows() {
	let pool = crate::create_pool("sqlite::memory:").await.unwrap();
	crate::run_migrations(&pool).await.unwrap();

	let now = 1_700_200_000_i64;
	// Old row beyond 48h retention gets pruned when newer merges arrive.
	record_merged_pull_request(&pool, 42, 1, "main", now - 60 * 3600)
		.await
		.unwrap();
	record_merged_pull_request(&pool, 42, 2, "main", now - 300)
		.await
		.unwrap();
	// Duplicate PR number is ignored cleanly.
	record_merged_pull_request(&pool, 42, 2, "main", now - 250)
		.await
		.unwrap();
	record_merged_pull_request(&pool, 42, 3, "main", now - 100)
		.await
		.unwrap();
	// Other repositories stay isolated.
	record_merged_pull_request(&pool, 99, 10, "main", now - 50)
		.await
		.unwrap();

	let timestamps = recent_merged_timestamps(&pool, 42, now - VELOCITY_LOOKBACK_SECS)
		.await
		.unwrap();
	assert_eq!(timestamps, vec![now - 300, now - 100]);

	// Verify old row was pruned from SQLite.
	let all_for_42 = recent_merged_timestamps(&pool, 42, 0).await.unwrap();
	assert_eq!(all_for_42, vec![now - 300, now - 100]);
}
