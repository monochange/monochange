---
"monochange_core": major
"monochange_publish": major
---

# Breaking publish API additions for the staged publish flow

> Constructing the affected structs or matching on the affected enums outside this workspace now requires updates. `PublishRequest`, `PublishSettings`, and `PackagePublicationTarget` gained a `flow` field, `PackagePublishStatus` gained a `Staged` variant, `PackagePublishSummary` gained a `staged` counter, and `PublishProgressEvent::RunFinished` gained a `staged` field.

Add `flow: PublishFlow::Direct` (or the desired flow) to every `PublishRequest`, `PublishSettings`, and `PackagePublicationTarget` struct literal:

```rust
// Before
PublishRequest { package_id, /* … */ mode, version, /* … */ }

// After
PublishRequest { package_id, /* … */ mode, flow: PublishFlow::Direct, version, /* … */ }
```

Handle the new `PackagePublishStatus::Staged` variant in every exhaustive match; a staged outcome means the upload succeeded but the version is not installable until a maintainer approves it with 2FA. `PackagePublishSummary::staged` defaults to `0` during deserialization, and `PublishProgressEvent::RunFinished { staged, .. }` can be ignored when only totals matter.

The serialized forms stay backward compatible: JSON reports and release records without the new fields deserialize with `flow = "direct"` and `staged = 0`.
