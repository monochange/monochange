use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::header::HeaderValue;

use super::RetryNotice;
use super::RetryReason;
use super::format_wait;
use super::retry_after;
use super::warn_retry;

fn notice(reason: RetryReason<'_>, wait: Duration) -> RetryNotice<'_> {
	RetryNotice {
		service: "GitHub",
		operation: "POST /repos/acme/widgets/git/trees",
		reason,
		wait,
		retry: 1,
		max_retries: 3,
	}
}

#[test]
fn rate_limit_notice_says_what_is_retried_and_when() {
	assert_eq!(
		notice(RetryReason::RateLimited, Duration::from_secs(60)).message(),
		"GitHub rate limit hit, retrying POST /repos/acme/widgets/git/trees in 1m (retry 1 of 3)"
	);
}

#[test]
fn failure_notice_names_the_failure() {
	assert_eq!(
		notice(RetryReason::Failed("HTTP 503"), Duration::from_secs(5)).message(),
		"GitHub request failed (HTTP 503), retrying POST /repos/acme/widgets/git/trees in 5s (retry 1 of 3)"
	);
}

#[test]
fn immediate_retry_says_now() {
	assert_eq!(
		notice(RetryReason::Failed("HTTP 502"), Duration::ZERO).message(),
		"GitHub request failed (HTTP 502), retrying POST /repos/acme/widgets/git/trees now (retry 1 of 3)"
	);
}

#[test]
fn warn_retry_emits_without_a_subscriber() {
	warn_retry(&notice(RetryReason::RateLimited, Duration::from_secs(1)));
}

#[test]
fn waits_render_in_the_largest_useful_units() {
	assert_eq!(format_wait(Duration::ZERO), "1s");
	assert_eq!(format_wait(Duration::from_millis(400)), "1s");
	assert_eq!(format_wait(Duration::from_secs(45)), "45s");
	assert_eq!(format_wait(Duration::from_secs(120)), "2m");
	assert_eq!(format_wait(Duration::from_secs(150)), "2m 30s");
	assert_eq!(format_wait(Duration::from_secs(3600)), "1h");
	assert_eq!(format_wait(Duration::from_secs(3600 + 5 * 60 + 9)), "1h 5m");
}

#[test]
fn retry_after_reads_whole_seconds() {
	let mut headers = HeaderMap::new();
	assert_eq!(retry_after(&headers), None);

	headers.insert("retry-after", HeaderValue::from_static(" 30 "));
	assert_eq!(retry_after(&headers), Some(Duration::from_secs(30)));

	headers.insert(
		"retry-after",
		HeaderValue::from_static("Wed, 21 Oct 2015 07:28:00 GMT"),
	);
	assert_eq!(retry_after(&headers), None);

	headers.insert("retry-after", HeaderValue::from_bytes(b"\xff").unwrap());
	assert_eq!(retry_after(&headers), None);
}
