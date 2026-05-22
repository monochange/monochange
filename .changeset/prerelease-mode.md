---
monochange_core: minor
monochange_config: minor
monochange: minor
monochange_schema: minor
monochange_changelog: patch
monochange_lint: patch
monochange_publish: patch
---

# Add prerelease mode

Add first-class prerelease configuration and release planning support.

Prerelease mode now writes `.monochange/prerelease-state.json`, preserves the original stable baseline across repeated prerelease preparations, supports planned/current/fixed stable bases, and can synthesize prerelease plans without changesets.

Validation now rejects stale prerelease state when prerelease mode is disabled, and stable release preparation removes the prerelease state file.

Enable incrementing alpha prereleases from the next planned stable version:

```toml
[prerelease]
enabled = true
channel = "alpha"
numbering = "increment"
base = "planned"
```

Use release-candidate prereleases from the current stable baseline when you want a tagged binary build without applying changeset bump severity yet:

```toml
[prerelease]
enabled = true
channel = "rc"
numbering = "increment"
base = "current-stable"
publish_packages = false
```

Use a fixed `0.0.0` nightly-style prerelease line with date-based identifiers:

```toml
[prerelease]
enabled = true
channel = "nightly"
numbering = "date"
base = "fixed"
base_version = "0.0.0"
keep_changesets = true
changelog = false
release_notes = true
publish_packages = false
```
