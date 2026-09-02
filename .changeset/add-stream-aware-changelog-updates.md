---
monochange_changelog: major
---

# render named changelog outputs from one audience stream

> **Breaking change:** `ChangelogUpdate` and `ReleaseNoteChange` now include `output` and/or `stream` fields. External callers that construct either type with a struct literal must supply those identities.

Changelog generation now partitions changes by their type's configured stream and renders each named output only from that stream. Existing package and group changelogs remain the implicit `default` output, while named outputs can append Markdown history or replace JSON, text, or Markdown files with the current release.

**Before:**

```rust
let update = ChangelogUpdate {
    file,
    owner_id,
    owner_kind,
    format,
    notes,
    rendered,
};
```

**After:**

```rust
let update = ChangelogUpdate {
    file,
    owner_id,
    owner_kind,
    output: "default".to_owned(),
    stream: "default".to_owned(),
    format,
    notes,
    rendered,
};
```

Add `stream: "default".to_owned()` to existing `ReleaseNoteChange` literals to retain their prior routing. Generated updates reject path collisions between different output identities, preventing two audiences from silently overwriting the same artifact.
