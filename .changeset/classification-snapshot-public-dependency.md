---
"monochange_classification": patch
---

# Accept `monochange_snapshot::SnapshotView` in the public snapshot helper

`monochange_classification` publicly depends on `monochange_snapshot`, and `run_package_snapshot` exposes a type from it directly:

```rust
pub fn run_package_snapshot(
	root: &Path,
	package_id: &str,
	save: bool,
	view: monochange_snapshot::SnapshotView,
) -> MonochangeResult<String>
```

`SnapshotView` is not re-exported from this crate, so callers must depend on `monochange_snapshot` to name the argument. That makes `monochange_snapshot` part of this package's consumer-visible surface rather than a private implementation detail: a breaking change to `SnapshotView` reaches callers of this function without any change here.

`monochange_snapshot` gained an opt-in `schema` feature and is released as `0.1.1`, so this changeset records the coupling for review. No signature changed and no caller needs to update anything. Pass `monochange_snapshot::SnapshotView::Index` for the compact surface used by the release workflow, or re-export the enum and re-classify once the analyzer models re-exports precisely.
