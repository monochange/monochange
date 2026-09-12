---
"monochange": patch
"monochange_config": patch
---

# Release Go modules from git tag baselines

Configured `type = "go"` packages could not be released: release planning dropped any package whose version is not stored in a manifest, and root-level manifests never matched their package definitions.

- `prepare-release` seeds the current version of tag-versioned ecosystems from the latest reachable tag matching the release owner's `version_format`, and warns when no matching tag exists instead of silently dropping the release.
- A configured package whose manifest sits at the workspace root (`path = "."`) now matches its definition; root manifests normalized to an empty relative directory and never matched, which also broke config-id changeset references for root npm, Deno, and Dart packages.
- Package definitions now match Python and Go discovery records, so version groups, changeset references, and release targets work for those ecosystems.
