---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_publish: minor
monochange_schema: minor
---

# add `publish.fail_on_duplicate` and keep already-published packages skipped by default

`monochange step publish-packages` now documents its default behavior explicitly: when a version is already published on the target registry, the package is skipped (`skipped_existing`) instead of failing the step. Only packages that genuinely cannot publish fail the step, so re-running a partially published release stays green.

A new per-package (and per-ecosystem) publish option opts into the strict behavior:

```toml
[package.pina_sdk_ids.publish]
fail_on_duplicate = true
```

With `fail_on_duplicate` enabled, a package whose version already exists on the registry (release mode only, including dry runs) is reported as `failed` with the message `… already exists on … and`publish.fail_on_duplicate`rejects duplicate version publications`, remaining packages are marked as not attempted, and the step exits non-zero. Placeholder publishing keeps its idempotent skip behavior regardless of the setting. The option flows from `monochange.toml` into release-record publication targets and the built-in `PublishPackages` step, and is documented in the configuration guide and the regenerated JSON Schemas.
