use chrono::DateTime;
use chrono::Utc;
use monochange_core::RegistryKind;

use super::*;

const CRATES_IO_429: &str = "    Updating crates.io index
   Packaging canvist_layout v0.0.0 (/tmp/.tmp97QCLt)
   Uploading canvist_layout v0.0.0 (/tmp/.tmp97QCLt)
error: failed to publish canvist_layout v0.0.0 to registry at https://crates.io

Caused by:
  the remote server responded with an error (status 429 Too Many Requests): You have published too many new crates in a short period of time. Please try again after Tue, 20 Oct 2026 10:59:10 GMT and see https://crates.io/docs/rate-limits for more details.";

fn at(timestamp: &str) -> DateTime<Utc> {
	DateTime::parse_from_rfc3339(timestamp)
		.unwrap_or_else(|error| panic!("timestamp {timestamp}: {error}"))
		.with_timezone(&Utc)
}

#[test]
fn crates_io_rejections_keep_the_reason_retry_time_and_limits_page() {
	let now = at("2026-10-20T10:53:00Z");
	let hit = detect_rate_limit_hit(RegistryKind::CratesIo, CRATES_IO_429, now)
		.unwrap_or_else(|| panic!("crates.io 429 should be recognised"));

	assert_eq!(
		hit,
		PublishRateLimitHit {
			reason: Some(
				"You have published too many new crates in a short period of time.".to_string()
			),
			retry_at: Some("2026-10-20T10:59:10+00:00".to_string()),
			docs_url: Some(CRATES_IO_RATE_LIMIT_DOCS.to_string()),
		}
	);
	assert_eq!(
		hit.render_message("crates_io", now),
		"rate limited by crates.io until 10:59:10 UTC (in 7 minutes). You have published too \
		 many new crates in a short period of time."
	);
}

#[test]
fn other_registries_are_recognised_from_their_429_wording() {
	let now = at("2026-10-20T10:00:00Z");

	let npm = detect_rate_limit_hit(
		RegistryKind::Npm,
		"npm error code E429\nnpm error 429 Too Many Requests - PUT https://registry.npmjs.org/pkg \
		 - Too many requests\nRetry-After: 120",
		now,
	)
	.unwrap_or_else(|| panic!("npm E429 should be recognised"));
	assert_eq!(npm.reason, None);
	assert_eq!(npm.retry_at.as_deref(), Some("2026-10-20T10:02:00+00:00"));
	assert_eq!(npm.docs_url, None);
	assert_eq!(
		npm.render_message("npm", now),
		"rate limited by npm until 10:02:00 UTC (in 2 minutes)."
	);

	let pypi = detect_rate_limit_hit(
		RegistryKind::Pypi,
		"HTTPError: 429 Too Many Requests from https://upload.pypi.org/legacy/\nToo many new \
		 projects created",
		now,
	)
	.unwrap_or_else(|| panic!("PyPI 429 should be recognised"));
	assert_eq!(pypi.reason, None);
	assert_eq!(pypi.retry_at, None);
	assert_eq!(pypi.render_message("pypi", now), "rate limited by PyPI.");

	let jsr = detect_rate_limit_hit(
		RegistryKind::Jsr,
		"error: Rate limit exceeded: publishing is limited per scope. Try again in 3 hours.",
		now,
	)
	.unwrap_or_else(|| panic!("JSR rate limit should be recognised"));
	assert_eq!(jsr.retry_at.as_deref(), Some("2026-10-20T13:00:00+00:00"));
	assert_eq!(jsr.docs_url.as_deref(), Some(JSR_PUBLISHING_DOCS));
	assert_eq!(
		jsr.render_message("jsr", now),
		"rate limited by JSR until 13:00:00 UTC (in about 3 hours). Rate limit exceeded: \
		 publishing is limited per scope."
	);

	let pub_dev = detect_rate_limit_hit(
		RegistryKind::PubDev,
		"Message from server: 429 rate limit reached. Try again in 30 seconds.",
		now,
	)
	.unwrap_or_else(|| panic!("pub.dev 429 should be recognised"));
	assert_eq!(
		pub_dev.retry_at.as_deref(),
		Some("2026-10-20T10:00:30+00:00")
	);
	assert_eq!(
		pub_dev.render_message("pub_dev", now),
		"rate limited by pub.dev until 10:00:30 UTC (in 1 minute). Message from server: 429 rate \
		 limit reached."
	);

	let minutes = detect_rate_limit_hit(
		RegistryKind::GoProxy,
		"429 too many requests; try again in 1 minute",
		now,
	)
	.unwrap_or_else(|| panic!("generic 429 should be recognised"));
	assert_eq!(
		minutes.retry_at.as_deref(),
		Some("2026-10-20T10:01:00+00:00")
	);
	assert_eq!(registry_display_name("go_proxy"), "the Go proxy");
	assert_eq!(registry_display_name("custom"), "custom");
}

#[test]
fn ordinary_failures_are_not_rate_limits() {
	let now = at("2026-10-20T10:00:00Z");

	assert_eq!(
		detect_rate_limit_hit(
			RegistryKind::CratesIo,
			"error: failed to verify package tarball\nerror[E0429]: `self` imports",
			now,
		),
		None
	);
	assert_eq!(
		detect_rate_limit_hit(RegistryKind::Npm, "npm error code E403", now),
		None
	);
}

#[test]
fn retry_times_read_as_a_clock_time_and_a_wait() {
	let hit = |retry_at: Option<&str>| {
		PublishRateLimitHit {
			reason: None,
			retry_at: retry_at.map(str::to_string),
			docs_url: None,
		}
	};
	let now = at("2026-10-20T10:00:00Z");

	assert_eq!(
		hit(Some("2026-10-20T11:00:00+00:00")).render_retry_at(now),
		Some("11:00:00 UTC (in 60 minutes)".to_string())
	);
	assert_eq!(
		hit(Some("2026-10-20T11:40:00+00:00")).render_retry_at(now),
		Some("11:40:00 UTC (in about 2 hours)".to_string())
	);
	assert_eq!(
		hit(Some("2026-10-22T09:00:00+00:00")).render_retry_at(now),
		Some("Thu 22 Oct 2026 09:00:00 UTC (in about 47 hours)".to_string())
	);
	assert_eq!(
		hit(Some("2026-10-20T09:00:00+00:00")).render_retry_at(now),
		Some("Tue 20 Oct 2026 09:00:00 UTC".to_string())
	);
	assert_eq!(hit(Some("soon")).render_retry_at(now), None);
	assert_eq!(hit(None).render_retry_at(now), None);
}
