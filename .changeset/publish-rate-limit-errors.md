---
monochange: feat
monochange_publish: feat
---

# Explain registry rate limits when publishing stops

When a registry refuses an upload with HTTP 429, `monochange publish` and `monochange publish placeholder` now say which registry stopped the run, why, when it accepts uploads again, and how to resume, instead of printing the tool's packaging log twice.

```text
✖ 🦀 cargo canvist_layout failed: rate limited by crates.io until 10:59:10 UTC (in 6 minutes). You have published too many new crates in a short period of time.
○ 🦀 cargo canvist_model not attempted because publishing canvist_layout 0.0.0 failed earlier in this run
✖ Publish complete: 11 published, 1 failed, 3 skipped
error[publish.rate_limited]: crates.io rate limit stopped publishing at canvist_layout 0.0.0: 11 of 15 packages published

    crates.io: You have published too many new crates in a short period of time.
    retry after: 10:59:10 UTC (in 6 minutes)
    limits: https://crates.io/docs/rate-limits

  command: monochange publish placeholder
  help:    Wait until 10:59:10 UTC (in 6 minutes), then rerun the same command. Versions that are already published are skipped, so publishing resumes with canvist_layout (4 packages left).
```

- Rate-limit rejections from crates.io, npm (`E429`), PyPI, JSR, and pub.dev are recognised from their 429 or "rate limit exceeded" wording. The retry time is read from `try again after <date>`, `Retry-After: <seconds>`, or `try again in <n> minutes`, and shown in UTC with the remaining wait.
- The diagnostic code is `publish.rate_limited` instead of `publish.failed`, and its hint tells you when to rerun.
- `PackagePublishOutcome` has a new optional `rate_limit` field (`PublishRateLimitHit { reason, retry_at, docs_url }`, with `retry_at` as an RFC 3339 UTC timestamp), so `--format json` reports and publish artifacts carry the same details. The tool's full output stays in the outcome's `stdout` and `stderr`.
- `monochange_publish` exports `detect_rate_limit_hit`, `registry_display_name`, `PublishRateLimitHit`, and `CRATES_IO_RATE_LIMIT_DOCS`.
