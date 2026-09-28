---
"monochange": fix
---

# Invalidate the implicit prepared-release cache when a changeset's bytes change

`monochange preview` could reuse `.monochange/local/prepared-release-cache.json` after a pending changeset was edited in place, so the reviewed plan (and the changelog rendered from it) kept the old severity and version. The cache recorded `head_commit`, worktree status lines, and content hashes for the _planned output_ paths only, so an edit to an untracked or already-dirty changeset left every signal unchanged.

The cache now stores a content fingerprint over the inputs the plan depends on — the pending changeset bytes and set, package manifests plus ancestor workspace manifests, `monochange.toml`, `.monochange/prerelease-state.json`, and release records — and rejects an implicit cache whose fingerprint differs. Rewriting a changeset's severity, editing only its body, adding or removing a changeset, and editing a manifest now all replan. An unchanged workspace still hits the cache, and an explicit `--prepared-release <PATH>` is still honored as a deliberate override.
