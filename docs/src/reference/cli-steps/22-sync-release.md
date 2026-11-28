# `SyncRelease`

## What it does

`SyncRelease` replays published release bookkeeping from frozen cut branches (`strategy = "cut"`) onto the current branch (`main`).

It inspects published release tags whose commits have not yet been synced onto `HEAD`, deletes their consumed `.changeset/*.md` files, prepends missing rendered changelog sections in chronological order, writes synced release records marked with `synced_from_commit`, and updates manifests unless `mode = "bookkeeping_only"`.

## Why use it

Use `SyncRelease` after a cut release branch (`release/<cut_id>`) has been tagged and published so `main` receives clean, conflict-free release bookkeeping without blocking ongoing feature development.

## Inputs

- `from`: optional release tag or commit SHA to include explicitly
- `mode`: optional override for `[source.pull_requests.sync].mode` (`semantic_pr`, `bookkeeping_only`, `merge_pr`, or `none`)
- `dry_run`: preview planned sync edits without modifying files on disk
- `format`: `text`, `markdown`, `json`, or `json-min`

## Side effects and outputs

- Deletes consumed `.changeset/*.md` files listed across unsynced published release records.
- Prepends missing rendered changelog entries in chronological order (oldest to newest).
- Writes `.monochange/releases/<hash>/release.json` with `synced_from_commit` set so post-merge release workflows on `main` never re-tag a sync commit.
- Updates manifests and configured versioned files in `semantic_pr` and `merge_pr` modes (skipping packages configured with `version_source = "tag"`).
- Exposes `sync.*` to later `Command` steps (such as lockfile regeneration or formatting) and `OpenSyncRequest`.

## Example

```bash
monochange step sync-release --dry-run --format json
monochange step sync-release --from v0.22.0 --mode merge_pr
```
