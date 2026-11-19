//! Tell people when monochange is waiting to retry a request.
//!
//! A rate-limited request can make a command sit silently for minutes while
//! it waits to retry. Every retry wait goes through [`warn_retry`], which
//! emits one `WARN` event. The CLI renders it as a `warning:` line above the
//! progress spinner (a `::warning` annotation in GitHub Actions), so a run
//! that is waiting says why and for how long instead of looking hung.

#[cfg(test)]
#[path = "__tests__/retry_tests.rs"]
mod tests;

use std::time::Duration;

use reqwest::header::HeaderMap;
use reqwest::header::RETRY_AFTER;

/// Why a request is being retried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryReason<'a> {
	/// The service answered with a rate limit (usually HTTP `429`, or `403`
	/// for a GitHub secondary rate limit).
	RateLimited,
	/// The request failed for another retryable reason, such as a `5xx`
	/// status or a dropped connection, described here.
	Failed(&'a str),
}

/// One upcoming retry: which service, what is retried, how long monochange
/// waits first, and which retry this is out of how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryNotice<'a> {
	/// The service that refused the request, such as `GitHub`.
	pub service: &'a str,
	/// What is being retried, such as `hosted CommitRelease`.
	pub operation: &'a str,
	pub reason: RetryReason<'a>,
	/// How long monochange waits before the retry.
	pub wait: Duration,
	/// This retry's number, from 1.
	pub retry: usize,
	/// How many retries monochange makes at most.
	pub max_retries: usize,
}

impl RetryNotice<'_> {
	/// The one-line message shown to people, such as
	/// `GitHub rate limit hit, retrying POST /repos/acme/widgets/git/trees in
	/// 1m (retry 1 of 3)`.
	#[must_use]
	pub fn message(&self) -> String {
		let Self {
			service,
			operation,
			reason,
			wait,
			retry,
			max_retries,
		} = self;
		let cause = match reason {
			RetryReason::RateLimited => format!("{service} rate limit hit"),
			RetryReason::Failed(detail) => format!("{service} request failed ({detail})"),
		};
		let when = if wait.is_zero() {
			"now".to_string()
		} else {
			format!("in {}", format_wait(*wait))
		};
		format!("{cause}, retrying {operation} {when} (retry {retry} of {max_retries})")
	}
}

/// Emit `notice` as a `WARN` event, right before the caller waits.
///
/// The event carries only its message, so it renders as one line.
pub fn warn_retry(notice: &RetryNotice<'_>) {
	tracing::warn!("{}", notice.message());
}

/// The wait a `Retry-After` header asks for, in whole seconds.
///
/// The HTTP-date form is not supported, since neither GitHub nor the
/// monochange app sends it. A missing or unreadable header gives `None`.
#[must_use]
pub fn retry_after(headers: &HeaderMap) -> Option<Duration> {
	headers
		.get(RETRY_AFTER)?
		.to_str()
		.ok()?
		.trim()
		.parse::<u64>()
		.ok()
		.map(Duration::from_secs)
}

/// Render a wait for people: `45s`, `2m`, `2m 30s`, `1h 5m`.
#[must_use]
pub fn format_wait(wait: Duration) -> String {
	let seconds = wait.as_secs().max(1);
	let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);

	match (hours, minutes, seconds) {
		(0, 0, seconds) => format!("{seconds}s"),
		(0, minutes, 0) => format!("{minutes}m"),
		(0, minutes, seconds) => format!("{minutes}m {seconds}s"),
		(hours, 0, _) => format!("{hours}h"),
		(hours, minutes, _) => format!("{hours}h {minutes}m"),
	}
}
