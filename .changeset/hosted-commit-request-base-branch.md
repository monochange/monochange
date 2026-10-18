---
monochange_core: breaking
---

# Add the base branch to `HostedCommitRequest`

**Breaking change:** `monochange_core::HostedCommitRequest` gains `base_branch: Option<String>`, the branch the release was prepared from. The monochange app uses it to refresh a release pull request only while that branch still points at the prepared commit, so a stale run never replaces a newer release.

Struct literals must add the field:

```rust
let request = HostedCommitRequest {
	// existing fields…
	base_branch: Some("main".to_owned()),
};
```

Serialized requests omit `base_branch` when it is `None`. See `docs/src/guide/migrations/0.19.md`.
