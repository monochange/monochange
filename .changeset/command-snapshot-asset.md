---
"monochange_schema": patch
---

# Commit the command snapshot schema asset

The generated schema asset set now includes `command-snapshot.schema.json` in both the canonical `crates/monochange_schema/schemas/` directory and the hosted `docs/src/schemas/` copy, plus a versioned `command-snapshot.v0.1.schema.json` during release preparation.

`cargo xtask schema update`, `schema:check`, `schema:release:update`, and `schema:release:check` all maintain the new asset, and its `$id` is <https://monochange.github.io/monochange/schemas/command-snapshot.schema.json>.

The asset is generated from the `monochange_snapshot` wire types, so it cannot drift from the document shape monochange accepts. Committed artifact fixtures and the schema asset inventory snapshot continue to describe the existing configuration and release-record kinds.
