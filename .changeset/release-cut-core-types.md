---
monochange_core: major
monochange_schema: major
---

# Add frozen release cut and sync configuration and wire types

> **Breaking change:** Rust callers constructing complete struct literals for `WorkspaceDefaults`, `ProviderMergeRequestSettings`, `ReleaseRecord`, `HostedCommitRequest`, or `HostedReleaseRequest` must initialize the new optional release-cut and sync fields (or use `..Default::default()`). Existing TOML configurations need no changes.

Adds `ReleasePullRequestStrategy`, `CutPullRequestSettings`, `CutTrigger`, `CutSourceCommit`, `CutBranchRetention`, `SyncPullRequestSettings`, `SyncMode`, `ActiveCutPullRequest`, and the `CliStepDefinition::SyncRelease` and `CliStepDefinition::OpenSyncRequest` step definitions.

Release records gain optional `cut_base_commit` and `synced_from_commit` fields. The release-record and config schema contracts advance from `0.10` to `0.11` with a no-op migration edge from `0.10` (both fields are optional and omitted when absent); published `v0.10` schema assets stay frozen.

Rust callers constructing settings can update struct literals with `..Default::default()`:

```rust
use monochange_core::{
    CutPullRequestSettings, ProviderMergeRequestSettings, ReleasePullRequestStrategy,
    SyncPullRequestSettings,
};

let settings = ProviderMergeRequestSettings {
    strategy: ReleasePullRequestStrategy::Cut,
    cut: CutPullRequestSettings::default(),
    sync: SyncPullRequestSettings::default(),
    ..Default::default()
};
```
