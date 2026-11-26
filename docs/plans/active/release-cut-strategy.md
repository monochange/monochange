# Frozen release cut strategy (`strategy = "cut"`) with sticky cuts

**Status**: implemented (ready for review) **Owner**: release automation

## Problem

High-velocity repositories with long CI run times (for example, 10 pull requests landing every 10 minutes with a 10-minute CI suite) livelock under the rolling release PR model because of two compounding races:

1. **Prep-time and head-branch churn**: Every push to `main` refreshes `monochange/release/release`. In `app/crates/monochange_app_api/src/github_app.rs` (`plan_release_branch_update`), if `main` moves while a run is preparing, the hosted backend returns `409 BaseMoved`. When a refresh does succeed, it overwrites the PR head and cancels/restarts 10-minute CI before it can finish.
2. **Merge-time base-branch churn**: `.github/workflows/release-pr-staleness.yml` and `monochange/actions` fast-forward merge require the release commit to sit strictly on top of `main`. Merging a stale release commit on top of newer `main` commits either conflicts on lockfiles or tags a tree that ships unlogged code alongside unconsumed `.changeset/*.md` files.

## Decision

Add an explicit release PR strategy (`strategy = "rolling"` default, `strategy = "cut"` opt-in) under `[source.pull_requests]`, with sticky auto-cuts, live status banners, explicit `/refresh`, tag/record-aware release planning, and non-blocking post-release sync modes (`semantic_pr`, `bookkeeping_only`, `merge_pr`, `none`):

1. **Sticky auto-cut**: When `main` has pending un-cut changesets and no Cut PR is open, `monochange` cuts a frozen base branch (`release/<cut_id>`) at a green commit `M*` on `main`, commits `C_rel` onto `monochange/release/<cut_id>`, and opens `monochange/release/<cut_id>` → `release/<cut_id>`. The PR diff shows strictly `M*..C_rel`.
2. **Frozen while open + status banner**: Subsequent pushes to `main` never overwrite the open Cut PR's branches; they only update an idempotent status banner (`<!-- monochange:cut-status:start -->` ... `<!-- monochange:cut-status:end -->`) reporting commits and changesets behind `main` plus the `/refresh` instruction.
3. **Explicit `/refresh`**: Authorized maintainers comment `/refresh` (or run `--refresh`) to re-cut from the newest green commit on `main`. Static `branch_template` values (such as `release/next`) update in place; versioned `branch_template` values (such as `release/{{ cut_id }}`) update in place when `<cut_id>` is unchanged or open a superseding Cut PR and close the old one when severity changes.
4. **Immediate ship from `release/<cut_id>`**: Merging the Cut PR fast-forwards `release/<cut_id>` to `C_rel`, which tags and publishes immediately without pausing `main`.
5. **Tag/record-aware planning**: Release planning resolves baselines from both `main` and published release tags/records, filters already-released changesets listed in published `ReleaseRecord.deleted_changesets`, carries forward un-synced changelog sections, and extends `previous_release_versioning` to inspect published tags so `vN+1` can always be cut before `vN` syncs back to `main`.
6. **Non-blocking sync back to `main` (`SyncRelease` + `OpenSyncRequest`)**:
   - `semantic_pr` (default): replay consumed changeset deletions, rendered changelogs, synced `release.json` (`synced_from_commit`), manifest versions, and freshly regenerated lockfiles onto `main` in one coalesced PR.
   - `bookkeeping_only`: pair with `[defaults].version_source = "tag"` (versionless `main`, e.g. `0.0.0-dev`) to sync only `.changeset/` deletions, changelogs, and `release.json` with zero manifest/lockfile edits.
   - `merge_pr`: build the same conflict-free tree on top of `main` as a two-parent bridge commit `(main_head, C_rel)` so merging the sync PR puts `(tag: vX.Y.Z)` directly into `git log` on `main` and preserves `git describe --tags`.
   - `none`: skip write-back and rely purely on tag-aware planning.

## Affected areas

- `crates/monochange_core/src/lib.rs` — strategy/cut/sync config types, `WorkspaceDefaults.version_source`, `ReleaseRecord` (`cut_base_commit`, `synced_from_commit`), `CliStepDefinition::{SyncRelease, OpenSyncRequest}`, hosted wire fields.
- `crates/monochange_config/src/lib.rs` — TOML parsing, defaults inheritance, and validation.
- `crates/monochange_hosting/src/lib.rs` & provider crates (`monochange_github`, `monochange_gitlab`, `monochange_gitea`, `monochange_forgejo`) — cut branch resolution, status banner rendering/patching, cut metadata parsing, latest-green lookup, active cut PR lookup.
- `crates/monochange/src/workspace_ops.rs` — tag/record-aware version baselines, consumed changeset filtering, changelog carry-forward, `previous_release_versioning` tag fallback.
- `crates/monochange/src/release_artifacts.rs`, `crates/monochange/src/cli_runtime.rs`, `crates/monochange/src/release_record.rs`, `crates/monochange/src/release_branch_policy.rs`, `crates/monochange/src/git_support.rs` — cut commit/PR execution, sticky banner updates, `SyncRelease` and `OpenSyncRequest`, synced-record tagging guard, deleted cut branch fallback.
- `crates/monochange_schema/` — versioned schema regeneration and migration (`0.10` → `0.11`).
- `app/crates/monochange_app_api/src/github_app.rs` & `release_api.rs` — ancestry-checked cut guard, cut-base ref creation, multi-parent commits, PR-body-only banner updates.
- `monochange/actions` — cut/refresh support, `sync-release` action variant, and post-merge chaining.

## Ordered checklist

- [x] Phase 1 — Core types, config parsing/validation, `ReleaseRecord` fields, and JSON schemas (`v0.11` with `0.10` → `0.11` migration).
- [x] Phase 2 — Tag/record-aware planning (`VersionSource::Tag` override, consumed-changeset filtering, changelog carry-forward, `previous_release_versioning` tag fallback) + integration tests.
- [x] Phase 3 — Frozen Cut PRs, sticky mode, status banner updates, and `--refresh` re-cutting (local + hosted).
- [x] Phase 4 — `SyncRelease` (`semantic_pr`, `bookkeeping_only`, `merge_pr` two-parent bridge commit, `none`), `OpenSyncRequest`, synced-record tagging guard, and deleted cut branch verification fallback.
- [x] Phase 5 — Hosted app (`app/crates/monochange_app_api`) and `monochange/actions` updates.
- [x] Phase 6 — Workflow templates, docs, changesets, and full validation (`cargo test`, `clippy`, `fmt`, `monochange step validate`, schema check, architecture check, cfg-test check).

## Acceptance checks

- [x] Existing `strategy = "rolling"` configs remain 100% backwards-compatible.
- [x] Sticky cut opens once from `M*`; subsequent runs without `--refresh` update only the status banner without mutating branches.
- [x] `--refresh` re-cuts from a newer green commit on `main`.
- [x] Cutting `vN+1` before `vN` syncs to `main` resolves the right next version, excludes `vN`'s consumed changesets, and carries forward `vN`'s changelog.
- [x] `semantic_pr`, `bookkeeping_only` + `version_source = "tag"`, and `merge_pr` (two-parent bridge commit with `vN` as ancestor of `main`) all pass tests.
- [x] Synced `ReleaseRecord` commits on `main` never trigger duplicate tagging.
- [x] All Rust unit tests, `monochange_integration_tests` snapshots, `app/` tests, `monochange/actions` tests, schema checks, and lints pass.
