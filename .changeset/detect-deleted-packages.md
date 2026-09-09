---
"@monochange/skill": minor
monochange: minor
monochange_analysis: minor
monochange_core: minor
monochange_semver: minor
---

# classify fully deleted packages as breaking changes

`monochange change classify` now discovers packages at both comparison endpoints. A pull request that deletes a package manifest reports the baseline package id, release owner, release tag, and removed API surface even when the candidate also removes its `monochange.toml` entry.

The report adds a high-confidence `monochange/package-lifecycle/package/removed/package` finding and proposes a major changeset. Package additions produce the corresponding minor finding. `monochange_core::SemanticChangeCategory` now includes `Package` for this lifecycle evidence.

Agents can use the standard command without a manual baseline-discovery fallback:

```bash
monochange change classify --format json --dependency-propagation public
```
