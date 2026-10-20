---
monochange_publish: major
---

# Record registry rate-limit rejections on publish outcomes

> Constructing `PackagePublishOutcome` outside this crate now requires the new `rate_limit` field.

`PackagePublishOutcome` gained `rate_limit: Option<PublishRateLimitHit>`, set when a registry refused the upload because of its rate limit. `PublishRateLimitHit` carries the registry's `reason`, `retry_at` (an RFC 3339 UTC timestamp), and `docs_url`. The field is skipped in JSON when it is `None`, so existing serialized reports still deserialize.

Add the field to every struct literal:

```rust
PackagePublishOutcome {
	// ...existing fields
	stderr: None,
	rate_limit: None,
}
```

New exports: `detect_rate_limit_hit(registry, output, now)` recognises a 429 or "rate limit exceeded" response in publish command output, `PublishRateLimitHit::render_message` and `render_retry_at` format it for people, `registry_display_name` maps `crates_io` to `crates.io` and similar, and `CRATES_IO_RATE_LIMIT_DOCS` links crates.io's limits page. `ensure_publish_report_succeeded` reports a rate-limited run as `<registry> rate limit stopped publishing at <package> <version>: …` with the reason, retry time, limits page, and a `help:` line.
