---
"monochange": patch
"monochange_config": patch
"monochange_core": patch
"monochange_go": patch
---

# Release Go modules from git tags

Configured Go modules could not be released: `prepare-release` failed discovery for every configured `type = "go"` package, and release planning had no way to resolve a baseline version for ecosystems whose versions live in git tags instead of a manifest field.

- Go modules now load as configured packages, so changesets can target them and version groups include them.
- Release planning resolves the current version of tag-versioned ecosystems from the latest reachable tag matching the release owner's `version_format`, and warns when no matching tag exists instead of silently dropping the release.
- A configured package whose manifest sits at the workspace root (`path = "."`) now matches its definition: root manifests normalized to an empty relative directory and never matched, which also broke config-id changeset references for root npm, Deno, and Dart packages.
- Package definitions now match Python and Go discovery records, so version groups, changeset references, and release targets work for those ecosystems.
