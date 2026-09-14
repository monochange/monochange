---
"monochange_snapshot": minor
---

# Publish a JSON Schema for command snapshots

`monochange_snapshot` now exposes its wire contract as JSON Schema, so CLIs in other ecosystems can validate the snapshot documents they emit before registering with `[package.<id>].cli`.

Enable the new `schema` feature and call `schema::command_snapshot`:

```rust
use monochange_snapshot::schema::command_snapshot;

let schema = command_snapshot().to_value();
```

The feature is additive and off by default, so existing extraction and diffing behavior is unchanged. The committed asset is published with the documentation site, and `xtask` generates it alongside the existing configuration and release-record schemas:

```text
https://monochange.github.io/monochange/schemas/command-snapshot.schema.json
https://monochange.github.io/monochange/schemas/command-snapshot.v0.1.schema.json
```

The published schema pins the `kind` discriminator to `cli-surface`, defaults `schema_version` to the current snapshot contract version, and sets `additionalProperties: false` on the document and every object definition so a misspelled field in a foreign emitter fails validation instead of being silently ignored.

The command snapshot contract versions independently of the `monochange_schema` configuration contract, so the versioned asset follows `SNAPSHOT_SCHEMA_VERSION` rather than the configuration schema version.
