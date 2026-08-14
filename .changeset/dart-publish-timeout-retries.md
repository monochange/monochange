---
monochange_core: minor
monochange_config: minor
monochange_publish: minor
monochange: patch
monochange_schema: patch
---

# Add a configurable publish timeout with retries and a Dart protected-publishing warning

- New `publish.timeout` settings (`timeout_seconds` default 60, `retries` default 2) cap how long a single package publish command may hang before it is killed and retried. Set `timeout_seconds = 0` to disable the timeout.
- Publish commands that time out are retried up to `retries` times; after the final attempt the package is reported as timed out instead of hanging the whole job.
- Dart/pub.dev packages using protected (trusted) publishing from a GitHub Actions `workflow_dispatch` event without a `PUB_TOKEN` fallback now emit a warning explaining that pub.dev automated publishing may require publishing from a pushed tag rather than a workflow dispatch

Configure the timeout per package or ecosystem:

```toml
[package.my-dart-package.publish.timeout]
timeout_seconds = 90
retries = 3
```
