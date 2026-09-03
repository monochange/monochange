---
monochange_config: major
monochange_schema: major
---

# configure simple audience-specific release-note streams

> **Breaking change:** callers that construct the public raw changelog configuration structs with literals must initialize the new stream and output fields. TOML users remain backward compatible because omitted routes resolve to the built-in `default` stream. The durable schema advances from `0.4` to `0.5`; immutable `v0.4` assets remain unchanged, and release records migrate through an explicit `0.4 -> 0.5` edge that assigns existing changelogs to the `default` output and stream.

Monochange configurations can now keep each changeset simple—package target, type, and Markdown body—while routing the whole file to exactly one audience stream. Types without a route continue to use the built-in `default` stream, so existing configurations and changesets keep their developer-facing behavior.

**Before:** one set of types fed every changelog destination.

```toml
[changelog.types]
feat = { bump = "minor", section = "features" }
```

**After:** custom types can select a stream, and named outputs render it for selected package or group targets.

```toml
[changelog.streams.user]
description = "Product release notes"

[changelog.types]
feat = { bump = "minor", section = "features" }
native = { bump = "major", section = "breaking" }
app_feature = { bump = "minor", section = "features", stream = "user" }

[changelog.outputs.user_json]
stream = "user"
path = "release-notes/{{ id }}/{{ version }}.json"
format = "json"
mode = "release"
targets = ["app"]
```

Each changeset must resolve to one stream. A file that combines a default-stream `native` target and a user-stream `app_feature` target fails validation with a concrete split-the-file diagnostic, keeping the audience decision explicit and auditable. The generated configuration schema includes streams, outputs, output modes, JSON/text formats, target validation, and the type-level `stream` key.
