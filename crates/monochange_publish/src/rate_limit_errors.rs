//! Recognise registry rate-limit rejections in publish command output.
//!
//! Registries answer a burst of uploads with HTTP 429 and usually say when to
//! try again. The raw tool output buries that between packaging logs, so this
//! module pulls out the registry's reason, the time uploads reopen, and the
//! registry's limits page, and renders them as one readable line.

use std::sync::LazyLock;

use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use monochange_core::RegistryKind;
use regex::Regex;
use serde::Deserialize;
use serde::Serialize;

use crate::rate_limits::JSR_PUBLISHING_DOCS;

/// crates.io's page describing its upload limits.
pub const CRATES_IO_RATE_LIMIT_DOCS: &str = "https://crates.io/docs/rate-limits";

/// A registry refused an upload because of its rate limit.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PublishRateLimitHit {
	/// The registry's own explanation, when it gave one.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub reason: Option<String>,
	/// When the registry accepts uploads again, as an RFC 3339 UTC timestamp.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub retry_at: Option<String>,
	/// The registry's documentation about its limits.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub docs_url: Option<String>,
}

static RETRY_AT_DATE: LazyLock<Regex> = LazyLock::new(|| {
	pattern(
		r"(?i)(?:try again|retry) after:? ([A-Za-z]{3}, \d{1,2} [A-Za-z]{3} \d{4} \d{2}:\d{2}:\d{2} GMT)",
	)
});
static RETRY_AFTER_SECONDS: LazyLock<Regex> =
	LazyLock::new(|| pattern(r"(?i)retry-after:?\s*(\d+)"));
static RETRY_IN: LazyLock<Regex> =
	LazyLock::new(|| pattern(r"(?i)try again in (\d+) (second|minute|hour)s?"));
static RETRY_SENTENCE: LazyLock<Regex> =
	LazyLock::new(|| pattern(r"(?i)\s*(?:please )?(?:try again|retry)\b.*$"));
static URL: LazyLock<Regex> = LazyLock::new(|| pattern(r"https?://[^\s)]+[^\s).,]"));

fn pattern(source: &str) -> Regex {
	Regex::new(source).unwrap_or_else(|error| panic!("invalid pattern {source}: {error}"))
}

/// Detect a rate-limit rejection in a failed publish command's output.
#[must_use]
pub fn detect_rate_limit_hit(
	registry: RegistryKind,
	output: &str,
	now: DateTime<Utc>,
) -> Option<PublishRateLimitHit> {
	let line = output.lines().find(|line| is_rate_limit_line(line))?;
	let docs_url = URL
		.find(line)
		.map(|url| url.as_str().to_string())
		.filter(|url| url.contains("rate"))
		.or_else(|| default_rate_limit_docs(registry).map(str::to_string));

	Some(PublishRateLimitHit {
		reason: registry_reason(line),
		retry_at: retry_at(output, now).map(|time| time.to_rfc3339()),
		docs_url,
	})
}

fn is_rate_limit_line(line: &str) -> bool {
	let line = line.to_lowercase();
	let too_many = line.contains("too many") || line.contains("rate limit");
	(line.contains("429") && too_many)
		|| line.contains("rate limit exceeded")
		|| line.contains("e429")
}

/// The sentence the registry wrote after the HTTP status, without the retry
/// instruction that monochange renders separately.
fn registry_reason(line: &str) -> Option<String> {
	let lowercase = line.to_lowercase();
	let start = lowercase
		.find("too many requests")
		.map_or(0, |index| index + "too many requests".len());
	let rest = line[start..].trim_start_matches([')', ':', ' ']);
	let rest = rest
		.strip_prefix("error: ")
		.or_else(|| rest.strip_prefix("Error: "))
		.unwrap_or(rest);
	let reason = RETRY_SENTENCE.replace(rest, "").trim().to_string();
	let looks_like_detail = !reason.is_empty()
		&& !reason.starts_with('-')
		&& !reason.starts_with("from ")
		&& !reason.to_lowercase().starts_with("npm error");
	looks_like_detail.then_some(reason)
}

fn retry_at(output: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
	if let Some(date) = RETRY_AT_DATE.captures(output) {
		return DateTime::parse_from_rfc2822(&date[1])
			.ok()
			.map(|time| time.with_timezone(&Utc));
	}
	if let Some(seconds) = RETRY_AFTER_SECONDS.captures(output) {
		return seconds[1]
			.parse::<i64>()
			.ok()
			.map(|seconds| now + Duration::seconds(seconds));
	}
	let delay = RETRY_IN.captures(output)?;
	let amount = delay[1].parse::<i64>().ok()?;
	let unit = match delay[2].to_lowercase().as_str() {
		"hour" => Duration::hours(amount),
		"minute" => Duration::minutes(amount),
		_ => Duration::seconds(amount),
	};
	Some(now + unit)
}

fn default_rate_limit_docs(registry: RegistryKind) -> Option<&'static str> {
	match registry {
		RegistryKind::CratesIo => Some(CRATES_IO_RATE_LIMIT_DOCS),
		RegistryKind::Jsr => Some(JSR_PUBLISHING_DOCS),
		_ => None,
	}
}

/// The registry name people use, such as `crates.io` for `crates_io`.
#[must_use]
pub fn registry_display_name(registry: &str) -> &str {
	match registry {
		"crates_io" => "crates.io",
		"pub_dev" => "pub.dev",
		"pypi" => "PyPI",
		"jsr" => "JSR",
		"go_proxy" => "the Go proxy",
		other => other,
	}
}

impl PublishRateLimitHit {
	/// When uploads reopen, such as `10:59:10 UTC (in 6 minutes)`.
	#[must_use]
	pub fn render_retry_at(&self, now: DateTime<Utc>) -> Option<String> {
		let time = DateTime::parse_from_rfc3339(self.retry_at.as_deref()?)
			.ok()?
			.with_timezone(&Utc);
		let wait = time - now;
		if wait <= Duration::zero() {
			return Some(time.format("%a %d %b %Y %H:%M:%S UTC").to_string());
		}
		let clock = if wait < Duration::days(1) {
			time.format("%H:%M:%S UTC").to_string()
		} else {
			time.format("%a %d %b %Y %H:%M:%S UTC").to_string()
		};
		Some(format!("{clock} (in {})", render_wait(wait)))
	}

	/// One line for logs and reports, such as
	/// `rate limited by crates.io until 10:59:10 UTC (in 6 minutes). You have
	/// published too many new crates in a short period of time.`
	#[must_use]
	pub fn render_message(&self, registry: &str, now: DateTime<Utc>) -> String {
		let mut message = format!("rate limited by {}", registry_display_name(registry));
		if let Some(retry_at) = self.render_retry_at(now) {
			message.push_str(" until ");
			message.push_str(&retry_at);
		}
		message.push('.');
		if let Some(reason) = &self.reason {
			message.push(' ');
			message.push_str(reason);
		}
		message
	}
}

fn render_wait(wait: Duration) -> String {
	let minutes = (wait.num_seconds() + 59) / 60;
	match minutes {
		..=1 => "1 minute".to_string(),
		2..=89 => format!("{minutes} minutes"),
		_ => format!("about {} hours", (minutes + 30) / 60),
	}
}

#[cfg(test)]
#[path = "__tests__/rate_limit_errors_tests.rs"]
mod tests;
