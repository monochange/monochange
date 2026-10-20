---
monochange_core: breaking
---

# Add a workflow-built tree to `HostedCommitRequest`

**Breaking change:** `monochange_core::HostedCommitRequest` gains `tree: Option<String>`, the git tree the workflow already created in the repository with its own `GITHUB_TOKEN`. When `tree` is set, `files` is empty and the monochange app only creates the commit and moves the release branch.

Struct literals must add the field:

```rust
let request = HostedCommitRequest {
	// existing fields…
	tree: None,
};
```

`files` now defaults to empty when deserializing and is omitted when empty, so a tree request carries no file contents. Servers that predate `tree` still require `files`, so they reject a tree request instead of committing an empty change. See `docs/src/guide/migrations/0.20.md`.
