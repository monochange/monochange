---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_schema: patch
---

# Add `auto_discover` to ecosystem configuration

Each `[ecosystems.*]` section now supports an `auto_discover` table that tells monochange to walk the workspace for packages matching `include` glob patterns. Discovered packages inherit defaults from the ecosystem and workspace, with explicit `[package.*]` entries taking priority.

```toml
[ecosystems.cargo]
enabled = true
versioned_files = ["Cargo.toml"]
auto_discover = { include = ["crates/*"] }

# Only packages that deviate from defaults need explicit entries:
[package.monochange_schema]
path = "crates/monochange_schema"
release = true
tag = true
```

The `auto_discover` table supports:

- `include` (required): glob patterns for directories to scan
- `exclude`: glob patterns to skip within included paths
- `id`: optional template for generated package IDs; defaults to `{{ name }}` and supports `name`, `path`, `sanitizedPath`, `manifest`, and `ecosystem` variables
- `defaults`: package-level defaults for all auto-discovered packages (`tag`, `release`, `version_format`)

Precedence: `[package.*]` explicit > `[ecosystems.*].auto_discover.defaults` > `[defaults]`
