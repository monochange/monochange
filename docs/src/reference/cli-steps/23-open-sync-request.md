# `OpenSyncRequest`

## What it does

`OpenSyncRequest` commits the files produced by `SyncRelease` (and any intermediate lockfile or formatting `Command` steps) onto `[source.pull_requests.sync].branch` (`monochange/sync-release` by default) and opens or updates a single coalesced sync pull request targeting `[source.pull_requests].base` (`main`).

When `sync.mode = "merge_pr"`, `OpenSyncRequest` constructs a multi-parent bridge commit `(main_head, C_rel_1, ...)` so merging the pull request puts every synced release tag directly into `main`'s Git ancestry without raw merge conflicts.

## Inputs

- `format`: `text`, `markdown`, `json`, or `json-min`
- `from`: optional release tag or commit SHA to sync explicitly when `OpenSyncRequest` runs standalone without a prior `SyncRelease` step
- `mode`: optional override for `[source.pull_requests.sync].mode` (`semantic_pr`, `bookkeeping_only`, `merge_pr`, or `none`)
- `no_verify`: skip local git hooks when committing and pushing
- `stage_all`: include files modified by intermediate `Command` steps (such as regenerated lockfiles)
- `backend`: `local` or `hosted`
- `hosted_auth`: `auto`, `oidc`, or `token`
- `hosted_url`: optional hosted app base URL override
- `oidc_audience`: optional GitHub Actions OIDC audience override

## Example

Single-invocation CLI usage (recommended for CI actions when no intermediate lockfile command is needed):

```bash
monochange step open-sync-request --from v0.22.0 --mode semantic_pr --format json
```

Multi-step workflow with intermediate lockfile regeneration:

```toml
[cli.sync-release]
help_text = "Replay published cut releases onto main and open the coalesced sync PR"

[[cli.sync-release.steps]]
type = "SyncRelease"

[[cli.sync-release.steps]]
type = "Command"
command = "cargo update --workspace"

[[cli.sync-release.steps]]
type = "OpenSyncRequest"
stage_all = true
```
