---
monochange_core: major
---

# add stream-aware release-note domain configuration

> **Breaking change:** externally constructible configuration and release structs now carry changelog stream and output identity. Callers that use struct literals must initialize the new fields; serde callers remain compatible because omitted fields resolve to the built-in `default` stream and output.

`ChangelogSettings` can now declare audience streams, route configured change types to one stream, and define named render outputs. `ReleaseManifestChangelog` records the selected stream/output, while `ProviderReleaseSettings` selects which output becomes a hosted release body.

**Before:**

```rust
let settings = ChangelogSettings {
    templates,
    sections,
    section_thresholds,
    types,
    style,
    release_notes,
};

let changelog = ReleaseManifestChangelog {
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
let settings = ChangelogSettings {
    templates,
    sections,
    section_thresholds,
    types,
    streams: BTreeMap::from([(
        DEFAULT_CHANGELOG_STREAM.to_owned(),
        ChangelogStreamDefinition::default(),
    )]),
    type_streams: BTreeMap::new(),
    outputs: BTreeMap::new(),
    style,
    release_notes,
};

let changelog = ReleaseManifestChangelog {
    owner_id,
    owner_kind,
    output: DEFAULT_CHANGELOG_OUTPUT.to_owned(),
    stream: DEFAULT_CHANGELOG_STREAM.to_owned(),
    path,
    format,
    notes,
    rendered,
};
```

Callers constructing `ProviderReleaseSettings` must also add `changelog_output`. Use `DEFAULT_CHANGELOG_OUTPUT.to_owned()` to preserve the previous hosted-release behavior. The new `ChangelogFormat::Json` and `ChangelogFormat::Text` variants, `ChangelogOutputDefinition`, and `ChangelogOutputMode` APIs provide structured and plain-text standalone release artifacts without changing SemVer planning.
