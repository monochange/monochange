# Frozen release cuts (`strategy = "cut"`)

When a repository ships at high velocity, the standard rolling release PR workflow can livelock. `strategy = "cut"` fixes that by separating **shipping a release** from **continuous merging on `main`**.

---

## Why rolling release PRs break down in high-frequency repositories

Suppose your CI suite takes **10 minutes** to run, and your team (or coding agents) merges **more than 1 pull request every 10 minutes** onto `main`:

```text
t =  0m  PR #101 merges -> main moves to M1 -> rolling Release PR updated -> CI starts (needs 10m)
t =  3m  PR #102 merges -> main moves to M2 -> rolling Release PR force-pushed -> CI CANCELED & restarts!
t =  7m  PR #103 merges -> main moves to M3 -> rolling Release PR force-pushed -> CI CANCELED & restarts!
t = 10m  PR #104 merges -> main moves to M4 -> rolling Release PR force-pushed -> CI CANCELED & restarts!
```

Even if CI somehow finishes on `M1`, fast-forward merging requires the release commit to sit directly on top of the current tip of `main`. Because `main` is already at `M4`, GitHub blocks the merge until the release PR is rebased—which restarts 10-minute CI all over again.

You are forced to either **freeze `main`** (blocking every developer) or **never ship**.

---

## The mental model in one sentence

> **Freeze a known-green snapshot of `main` on a short-lived release branch, run CI once, ship immediately from that branch, and sync the release bookkeeping back to `main` asynchronously.**

Developers never wait for releases, and releases never wait for developers.

---

## Visual workflow: how branches, PRs, tags, and sync fit together

```text
main (never pauses — PRs merge continuously)
  M1 (green) ──► M2 ──► M3 ──► M4 ──► M5 ──► M6 ──► S1 (sync PR merged) ──► M7 ...
   │                                                  ▲
   │ 1. Auto-cut once from latest green M1            │ 5. Non-blocking Sync PR
   │    (creates frozen base `release/v1.1.0` at M1)  │    (`monochange/sync-release` -> `main`)
   ▼                                                  │    Replays: deleted .changeset/, changelog,
release/v1.1.0  M1 ─────────────────────────► C_rel   │    synced release.json, optional manifests
                  \                          ▲  │     │
                   \  2. Cut PR opens:       │  │     │    (in `merge_pr` mode, S1 is a clean
                    └─ monochange/release/   │  │     │     two-parent bridge commit (M6, C_rel)
                       v1.1.0 (C_rel) ───────┘  │     │     so `v1.1.0` appears in `git log main`
                       • Diff is ONLY M1..C_rel │     │     with ZERO lockfile conflicts!)
                       • Pushes to main (M2..M5)│     │
                         update ONLY the status │     │
                         banner — 0 CI restarts!│     │
                                                │     │
                       3. CI goes green -> merge│     │
                          Cut PR into           │     │
                          release/v1.1.0        │     │
                                                ▼     │
                       4. Tag `v1.1.0` on C_rel ──────┘
                          Publish crates/npm/GitHub release
                          Delete `release/v1.1.0` (retention = "delete")
```

---

## Step-by-step walkthrough

### Step 1: Sticky auto-cut from a green commit (`M*`)

When `main` has pending `.changeset/*.md` files and **no Cut PR is open**, `monochange` automatically:

1. Finds the newest commit `M*` on `main` whose GitHub checks/statuses are green (`source_commit = "latest_green"`).
2. Creates a frozen base branch at `M*` — by default `release/<cut_id>` (for example `release/v1.1.0`).
3. Prepares the release on top of `M*` (bumping versions, updating changelogs and lockfiles, deleting consumed changesets, writing `.monochange/releases/<id>/release.json` with `cut_base_commit = M*`) and commits `C_rel` to `monochange/release/v1.1.0`.
4. Opens the **Cut PR**: `monochange/release/v1.1.0` → `release/v1.1.0`.

Because `release/v1.1.0` starts at `M*`, the PR diff contains **only** the release commit (`M*..C_rel`) — never unrelated feature commits from `main`.

### Step 2: Stay frozen while `main` keeps moving (live status banner)

While the Cut PR is open, PRs #102, #103, #104… keep merging into `main` at full speed.

On each push to `main`, `monochange` sees that a Cut PR is already open (`trigger = "sticky"`). It does **not** touch `release/v1.1.0` or `monochange/release/v1.1.0`, does **not** run lockfile commands, and does **not** restart CI. Instead, it updates an idempotent HTML-comment status banner at the top of the Cut PR description:

```markdown
<!-- monochange:cut-status:start -->
> **Frozen release cut `v1.1.0`** from `main` (`abc1234`) — **4 newer commits** (2 changesets) on `main`.
> Comment `/refresh` to re-cut from the newest green commit, or merge this PR as-is to ship `v1.1.0`.
<!-- monochange:cut-status:end -->
```

### Step 3: Optional `/refresh` on demand

If a maintainer *wants* to pull newer changes from `main` into the pending release before shipping, they comment `/refresh` on the Cut PR (or run `monochange run release --refresh`):

- `monochange` verifies the commenter has write/maintain/admin permission.
- It cuts fresh from the newest green commit on `main`.
- If the release version stays `v1.1.0`, it updates `release/v1.1.0` and `monochange/release/v1.1.0` in place.
- If a new breaking or minor changeset bumps the version (for example `v1.1.0` → `v1.2.0`), it opens the new `v1.2.0` Cut PR, comments on the old `v1.1.0` PR linking to the replacement, and closes the old PR cleanly.

### Step 4: Merge the Cut PR → tag & publish immediately

Once CI passes on the Cut PR, click **Merge** (fast-forward or merge into `release/v1.1.0`).

- `release/v1.1.0` moves to `C_rel`, which triggers your post-merge release job immediately.
- `monochange step tag-release` creates and pushes `v1.1.0` pointing directly at `C_rel` (the exact commit whose CI just passed!).
- `monochange step publish-packages` and `monochange step publish-release` publish your packages and GitHub Release.
- When `retention = "delete"` (default), `PublishRelease` deletes the temporary `release/v1.1.0` branch automatically once published. The `v1.1.0` tag stays permanent, and `verify_release_ref_for_publish` uses `cut_base_commit` ancestry on `main` so retries or re-runs still pass after the branch is gone.

### Step 5: Non-blocking sync back to `main`

After tagging and publishing, `monochange` runs `SyncRelease` + `OpenSyncRequest` to catch `main` up cleanly in a single PR (`monochange/sync-release` → `main`):

- Deletes the `.changeset/*.md` files already shipped in `v1.1.0`.
- Inserts the rendered changelog section(s) cleanly at the top of `CHANGELOG.md`.
- Writes the synced `.monochange/releases/<id>/release.json` marked with `synced_from_commit = "<C_rel>"`.
- Updates manifest versions & lockfiles cleanly on top of current `main` (depending on `sync.mode`).

**Important:** `TagRelease` and `monochange/actions/post-merge-release` inspect `synced_from_commit` and **refuse to re-tag** synced bookkeeping commits when the sync PR merges to `main`.

---

## Frequently asked questions

### What if we cut `v1.2.0` before `v1.1.0`'s sync PR finishes merging into `main`?

It works automatically! Under `strategy = "cut"`, release planning is **tag- and record-aware**:

1. **Baselines**: `monochange` computes each package's starting version as `max(manifest_version, latest_published_tag)` (or strictly from tags when `[defaults].version_source = "tag"`). So even though `main`'s `Cargo.toml` still says `1.0.0`, `monochange` sees the `v1.1.0` tag and plans `1.2.0`.
2. **Consumed changesets**: `monochange` reads `deleted_changesets` from unsynced published release records (`v1.1.0`), ignores those stale changesets when planning `v1.2.0`, and deletes their stale copies from the `v1.2.0` cut branch.
3. **Changelogs**: Unsynced rendered changelog entries from `v1.1.0` are carried forward chronologically so `v1.2.0`'s `CHANGELOG.md` shows `1.2.0` → `1.1.0` → `1.0.0` without losing history.
4. **Coalesced sync**: When `SyncRelease` runs next, it replays every unsynced published tag (`v1.1.0`, `v1.2.0`, …) in chronological order into a single clean sync PR.

### Does the `release/v1.1.0` branch clutter the repository?

No. By default, `cut.retention = "delete"` deletes `release/v1.1.0` automatically right after `PublishRelease` succeeds. Set `retention = "keep"` only if you intentionally maintain long-lived release branches (for example `release/v1.x` backport branches).

### Will `git log main` still show `(tag: v1.1.0)`?

That depends on the `sync.mode` you choose:

| Mode | Manifests on `main` | Lockfiles on `main` | `(tag: vX.Y.Z)` in `git log main` | Best for |
| --- | --- | --- | --- | --- |
| `semantic_pr` (default) | Updated cleanly | Regenerated cleanly via `Command` steps | No (linear history) | Repos using squash/rebase or GitHub Merge Queue with versioned manifests |
| `merge_pr` | Updated cleanly | Regenerated cleanly via `Command` steps | **Yes** (two-parent bridge commit) | Repos that allow merge commits and want `(tag: vX.Y.Z)` in `git log main` and working `git describe --tags` — **with zero raw lockfile conflicts** |
| `bookkeeping_only` | Untouched (`0.0.0-dev`) | Untouched | No | Highest-velocity repos paired with `[defaults].version_source = "tag"` (zero manifest/lockfile churn on `main`) |
| `none` | Untouched | Untouched | No | Tag-driven repos that prefer no write-back PRs at all |

#### How `merge_pr` gives you `(tag: v1.1.0)` in `git log main` without merge conflicts

A raw `git merge release/v1.1.0` into a moved `main` almost always hits painful lockfile conflicts (`Cargo.lock`, `pnpm-lock.yaml`).

Instead, `sync.mode = "merge_pr"` builds the **exact same clean, freshly regenerated tree** as `semantic_pr` on top of current `main`, and writes it as a **two-parent bridge commit** whose parents are `(main_head, C_rel)`. When the sync PR merges with a merge commit, `C_rel` (and its `v1.1.0` tag) becomes a true Git ancestor of `main` — so `git log --decorate` and `git describe --tags` show every release tag, with zero raw merge conflicts.

---

## Configuration (`monochange.toml`)

```toml
[defaults]
# Optional: set to "tag" for versionless `main` (`0.0.0-dev` in manifests) paired
# with `sync.mode = "bookkeeping_only"`.
# version_source = "manifest"

[source]
provider = "github"
owner = "acme"
repo = "monorepo"

[source.releases]
enabled = true
branches = ["release/*"]
enforce_for_tags = true
enforce_for_publish = true

[source.pull_requests]
enabled = true
strategy = "cut"
base = "main"
branch_prefix = "monochange/release"
title = "chore(release): prepare release {{ cut_id }}"
labels = ["release", "automated"]

[source.pull_requests.cut]
branch_template = "release/{{ cut_id }}"
trigger = "sticky"             # "sticky" (default) | "manual"
source_commit = "latest_green" # "latest_green" (default) | "head"
refresh_command = "/refresh"
status_banner = true
retention = "delete"           # "delete" (default) | "keep"

[source.pull_requests.sync]
mode = "semantic_pr"           # "semantic_pr" | "bookkeeping_only" | "merge_pr" | "none"
branch = "monochange/sync-release"
title = "chore(release): sync {{ cut_id }} to {{ base }}"
labels = ["release", "automated", "release-sync"]
auto_merge = true
```

## GitHub Actions workflow (`.github/workflows/release-cut.yml`)

```yaml
name: Release Cuts

on:
  push:
    branches:
      - main
      - release/*
  issue_comment:
    types: [created]
  workflow_dispatch:
    inputs:
      refresh:
        description: Re-cut the open release PR from the newest green commit
        type: boolean
        default: false
      from:
        description: Optional base commit SHA or ref to cut from
        type: string
        default: ""

permissions:
  contents: write
  pull-requests: write
  issues: write
  id-token: write

jobs:
  cut-or-banner:
    if: >-
      (github.event_name == 'push' && github.ref == 'refs/heads/main') ||
      github.event_name == 'workflow_dispatch' ||
      (github.event_name == 'issue_comment' && github.event.issue.pull_request != null)
    runs-on: ubuntu-latest
    concurrency:
      group: monochange-release-cut
      cancel-in-progress: false
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Cut frozen release PR or update status banner
        uses: monochange/actions@v0
        with:
          name: release-pr
          refresh: ${{ inputs.refresh || false }}
          from: ${{ inputs.from || '' }}
          minimum-reviewer-permission: write

  tag-publish-and-sync:
    if: github.event_name == 'push' && startsWith(github.ref, 'refs/heads/release/')
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Tag, publish, and sync back to main
        uses: monochange/actions@v0
        with:
          name: post-merge-release
          sync-release: true
          open-next-cut: true
```

---

## Automatic suggestions in `monochange.dev`

When your repositories are connected to the monochange GitHub App, `monochange.dev` monitors merge velocity across every rolling 24-hour window. If any repository merges **more than 1 pull request every 10 minutes on average for at least an hour** (7 or more merged PRs inside any 60-minute window) and is not yet using `strategy = "cut"`, your `/dashboard` surfaces a recommendation card with two options:

1. **Open setup PR**: creates a ready-to-review pull request (`monochange/enable-release-cuts`) that patches `monochange.toml` and adds `.github/workflows/release-cut.yml` automatically.
2. **Copy prompt for agent**: copies a repository-specific migration prompt you can paste directly into Claude Code, Cursor, Codex, or any coding agent.

## Copy-paste prompt for your coding agent

If you want a coding agent to migrate an existing repository manually, paste this prompt:

```text
Switch this repository from rolling release PRs to monochange's frozen release cut workflow (`strategy = "cut"`).

1. Update `monochange.toml` to enable `[source.releases]` (`branches = ["release/*"]`), `[source.pull_requests]` (`strategy = "cut"`), `[source.pull_requests.cut]` (`branch_template = "release/{{ cut_id }}"`, `trigger = "sticky"`, `source_commit = "latest_green"`, `refresh_command = "/refresh"`, `status_banner = true`, `retention = "delete"`), and `[source.pull_requests.sync]` (`mode = "semantic_pr"`, or `"merge_pr"` if this repo uses merge commits and wants tags in `git log main`).
2. Add or update `.github/workflows/release-cut.yml` using `monochange/actions` (`name: release-pr` for pushes to `main` + `/refresh` comments, and `name: post-merge-release` with `sync-release: true` and `open-next-cut: true` for pushes to `release/*`).
3. Preserve all existing package/group definitions, lockfile commands, and publish steps.
4. Verify with `monochange step validate` and `monochange run release --dry-run`.
```
