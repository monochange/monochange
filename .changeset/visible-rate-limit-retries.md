---
monochange: fix
monochange_github: fix
monochange_forgejo: fix
monochange_hosting: feat
---

# Show rate-limit retries instead of waiting silently

A command that hit a rate limit used to sit without output while it waited to retry, so a release job looked hung for minutes. Every retry wait is now announced as a warning just before monochange waits, with what is retried, how long the wait is, and which retry it is:

```text
warning: GitHub rate limit hit, retrying GET /repos/acme/widgets/releases/tags/v1.2.0 in 1m (retry 1 of 3)
warning: monochange app rate limit hit, retrying hosted CommitRelease in 30s (retry 1 of 3)
```

In GitHub Actions each line is a `::warning` annotation, and with `--progress-format json` it is a `warning` event. `--quiet` hides them like any other warning.

**GitHub API (`monochange_github`).** Clients now use octocrab's `RetryConfig::HandleRateLimits`. A `429`, or a `403` that carries `retry-after` or `x-ratelimit-remaining: 0`, waits as long as GitHub asks (at least a minute for a `429` with no hint) and is retried up to three times. Before, a `429` was retried immediately three times, which GitHub's guidance warns can get an integration banned, and a primary rate limit failed at once. A `5xx` is still retried immediately and is now announced. Release tree writes keep their own capped backoff from the previous release, and changeset enrichment still skips rather than waits. GitHub clients also gain a 10 second connect timeout and a 60 second read timeout, so a stalled connection fails instead of hanging the command.

**Hosted operations (`monochange`).** When the monochange app answers `CommitRelease`, `OpenReleaseRequest`, or `monochange gh workflow run` with `429`, the CLI waits for the app's `Retry-After` (30 seconds without one) and sends the request again, up to three times, over RPC and over the REST fallback. A `Retry-After` longer than five minutes fails at once. Either way the error says what happened:

```text
hosted CommitRelease failed with HTTP 429 Too Many Requests: rate_limited. The monochange app kept rate limiting this request after 3 retries; rerun the job in a few minutes
```

The GitHub Actions OIDC token request now also retries a `429`, and its existing retries after a `5xx` or a dropped connection are announced.

**Forgejo (`monochange_forgejo`).** Release and pull request calls now use the shared provider client with its 10 second connect and 30 second request timeouts, like GitLab and Gitea. Before, they had no timeout and could wait forever on an unresponsive server.

**`monochange_hosting`.** The new `retry` module holds the shared pieces: `warn_retry(&RetryNotice { .. })` emits the warning, `retry_after(&headers)` reads a `Retry-After` in seconds, and `format_wait` renders a wait such as `2m 30s`.
