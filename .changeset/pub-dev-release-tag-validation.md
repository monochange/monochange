---
monochange: major
---

# Require a release tag for trusted pub.dev packages

> **Breaking change:** `monochange step validate` and `monochange publish readiness` now fail for a pub.dev package that publishes with trusted publishing but whose releases create no tag, and `monochange publish packages` refuses to start a real run for it. See [the 0.23 migration guide](https://monochange.github.io/monochange/guide/migrations/0.23.html).

pub.dev only accepts a trusted upload from a run on the package's own release tag, so a package without one could never publish. The error names the section to change:

```text
error[config.invalid]: workspace validation failed

    `app_dart` publishes to pub.dev with trusted publishing, which only accepts an upload from a run on the package's release tag (`refs/tags/<tag>`), but monochange creates no tag for it. Set `tag = true` on `[package.app_dart]` so every release creates the tag, or set `publish.trusted_publishing = false` on `[package.app_dart]` to publish with a pub.dev token instead.
```

Set `tag = true` on the package, or on its group when the package belongs to one:

```toml
[package.app_dart]
path = "packages/app_dart"
type = "dart"
tag = true
```

Validation reads only `monochange.toml`, so a package that publishes with a `PUB_TOKEN` must also set `publish.trusted_publishing = false` to pass. Packages that set `publish.trusted_publishing = false`, use `publish.mode = "external"`, or are private (`publish_to: none` in `pubspec.yaml`) are unaffected.
