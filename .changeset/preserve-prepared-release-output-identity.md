---
monochange: major
---

# preserve release-note output identity in prepared releases

> **Breaking change:** `PreparedChangelog` adds public `output` and `stream` fields. Callers that construct it directly must initialize both fields. Existing serialized prepared releases remain readable because missing identities deserialize as `default`.

Prepared releases and release records now retain the named destination and audience stream for every changelog artifact. That identity lets later commit and hosted-release steps select the intended notes deterministically instead of guessing from a path or taking the first changelog for a target.

**Before:**

```rust
let changelog = PreparedChangelog {
    owner_id,
    owner_kind,
    path,
    format,
    notes,
    rendered,
};
```

**After:**

```rust
let changelog = PreparedChangelog {
    owner_id,
    owner_kind,
    output: "default".to_owned(),
    stream: "default".to_owned(),
    path,
    format,
    notes,
    rendered,
};
```

Use `default` for both new fields when migrating callers that want the existing single-changelog behavior. Release records written before this change are normalized to the same identities when they are loaded.
