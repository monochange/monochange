---
monochange_core: minor
monochange_config: minor
monochange: minor
---

# Add prerelease mode

Add first-class prerelease configuration and release planning support.

Prerelease mode now writes `.monochange/prerelease-state.json`, preserves the original stable baseline across repeated prerelease preparations, supports planned/current/fixed stable bases, and can synthesize prerelease plans without changesets.

Validation now rejects stale prerelease state when prerelease mode is disabled, and stable release preparation removes the prerelease state file.
