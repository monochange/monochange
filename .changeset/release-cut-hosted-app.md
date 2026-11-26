---
monochange_app: minor
---

# Support frozen release cuts, multi-parent sync commits, and banner updates in the hosted app

The monochange GitHub App now accepts cut release branches (`<prefix>/release/<cut_id>`), sync branches (`<prefix>/sync-release`), and frozen cut base branches (`release/<cut_id>`), validates base commit ancestry via GitHub's compare API when `allow_base_ancestor` is set, supports multi-parent bridge commits (`additional_parents`) for `sync.mode = "merge_pr"`, patches sticky Cut PR status banners without mutating branches (`banner_only`), and comments on and closes HMAC-verified superseded Cut PRs (`supersede_pr_number`).

In `monochange.dev`, connected repositories that merge more than 1 PR every 10 minutes on average for at least an hour within a 24-hour window (7+ merged PRs in any 60-minute sliding window) now surface a dashboard suggestion card offering both a one-click **Open setup PR** action (`monochange/enable-release-cuts`) and a **Copy prompt for agent** button with an inline preview. The documentation book now includes an in-depth frozen release cuts guide with diagrams and `SyncRelease` / `OpenSyncRequest` step references.
