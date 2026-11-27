use chrono::DateTime;
use chrono::Utc;
use monochange_core::RegistryKind;

use super::*;

fn at(timestamp: &str) -> DateTime<Utc> {
	DateTime::parse_from_rfc3339(timestamp)
		.unwrap_or_else(|error| panic!("timestamp {timestamp}: {error}"))
		.with_timezone(&Utc)
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
