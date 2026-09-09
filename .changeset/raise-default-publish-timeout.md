---
"monochange_core": patch
"monochange_config": patch
monochange_schema: major
"monochange": patch
"@monochange/cli": patch
---

# Raise the default publish timeout from 60 to 300 seconds

`cargo publish` waits for crates.io's server-side verification to complete, and that can take minutes when the registry queue is backed up. During the monosecret v0.3.3 release, `cargo publish --locked -p monosecret` needed ~4 minutes and exhausted all three 60-second attempts, failing the release's publish step (and skipping `monosecret_derive`, which depends on the crate being present first).

The default `publish.timeout.timeout_seconds` is now `300` seconds. The timeout is a ceiling, not a delay: publishes that finish quickly are unaffected, while slow registries no longer report spurious failures (or leave a package unpublished after a run that actually succeeded server-side).

The schema default is part of the published contract, so the release-record and configuration schemas advance to `v0.6`; the `0.5` → `0.6` migration edge accepts existing records unchanged. Override the default per ecosystem or per package, and set `timeout_seconds = 0` to disable the timeout entirely:

```toml
[ecosystems.cargo.publish.timeout]
timeout_seconds = 300
retries = 2
```
