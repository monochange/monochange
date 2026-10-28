---
monochange: perf
monochange_analysis: patch
---

# Batch analysis snapshots and analyze packages in parallel

`monochange analyze` and `monochange change classify` now spend far fewer git processes per run:

- Snapshot labels are resolved once per target instead of once per package. A working-tree label stages the entire tree into a temporary index, so a 100-package workspace previously repeated that staging 100 times per side of the comparison.
- Index snapshots and discovery materialization read every file through a single `git cat-file --batch` run instead of one `git show` process per file, matching what committed-revision snapshots already did.
- Packages are analyzed in parallel across CPU cores. Each package's analysis may spawn an ecosystem subprocess (`node` for TypeScript, `cargo semver-checks` for Cargo), which is exactly the work worth spreading; result order stays deterministic.
- Release trajectory analysis (`--release-ref`/`--main-ref`/`--head-ref`) shares one analysis session across its three comparison ranges, so discovery runs once per git tree instead of once per range.

On workspaces with many packages, classification time drops roughly with core count plus the eliminated per-package git round trips; single-package workspaces are unchanged.
