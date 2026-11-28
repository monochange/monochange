---
monochange: minor
monochange_config: minor
monochange_changelog: minor
monochange_hosting: minor
monochange_github: minor
---

# Add sticky frozen release cuts and non-blocking post-release sync

High-velocity repositories where `main` advances faster than CI run times can now set `[source.pull_requests].strategy = "cut"` to cut a frozen release branch (`release/<cut_id>`) from a green commit on `main`, keep the Cut PR undisturbed while updating an idempotent status banner on subsequent pushes to `main`, and explicitly re-cut on demand with `--refresh` or `/refresh`.

Adds workspace-wide `[defaults].version_source` (`"manifest"` or `"tag"`), tag- and record-aware release planning (skipping changesets consumed by unsynced published tags and carrying forward rendered changelog entries chronologically), and the `SyncRelease` and `OpenSyncRequest` CLI steps supporting `semantic_pr`, `bookkeeping_only`, `merge_pr` (two-parent bridge commits without raw merge conflicts), and `none` sync modes.

```toml
[source.releases]
branches = ["release/*"]

[source.pull_requests]
strategy = "cut"
base = "main"

[source.pull_requests.cut]
branch_template = "release/{{ cut_id }}"
trigger = "sticky"
source_commit = "latest_green"

[source.pull_requests.sync]
mode = "semantic_pr"
```
