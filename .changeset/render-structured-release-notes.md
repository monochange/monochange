---
monochange: feat
monochange_changelog: feat
monochange_config: feat
monochange_core: feat
---

# Keep release notes structured until their output format is known

JSON release-note artifacts now expose summaries, Markdown details, package labels, change types, bump severity, streams, layout, and provenance as separate fields. Text artifacts render those fields directly without Markdown emphasis, while Markdown uses compact list entries for routine changes and expanded sections for breaking changes, migrations, code blocks, and multi-paragraph explanations.

Package changelogs omit their own package label. Group and workspace changelogs retain package labels so readers can see where each change applies. The built-in section names no longer include emoji; repositories can still opt in by putting emoji in configured section headings.

The public document types remain source-compatible through a default generic entry type. Callers can opt into structured entries and convert them for an adapter that still expects Markdown strings:

```rust
// Before: entries were rendered before choosing JSON or text.
let notes: ReleaseNotesDocument = ReleaseNotesDocument {
    title: "1.2.3".into(),
    summary: vec![],
    sections: vec![ReleaseNotesSection {
        title: "Fixes".into(),
        collapsed: false,
        entries: vec!["- Keep JSON failures non-zero".into()],
    }],
};

// After: the existing form still works, while typed entries are available.
let notes: ReleaseNotesDocument<ReleaseNotesEntry> = ReleaseNotesDocument {
    title: "1.2.3".into(),
    summary: vec![],
    sections: vec![ReleaseNotesSection {
        title: "Fixes".into(),
        collapsed: false,
        entries: vec![ReleaseNotesEntry {
            summary: "Keep JSON failures non-zero".into(),
            details_markdown: Some("CI can trust the exit status.".into()),
            packages: vec![],
            change_type: Some("fix".into()),
            bump: BumpSeverity::Patch,
            stream: "default".into(),
            style: ReleaseNoteEntryStyle::Compact,
            provenance: ReleaseNoteProvenance::default(),
        }],
    }],
};
let legacy = notes.to_legacy_markdown(&ChangelogStyle::default());
```

The `changesets/recommended` preset now requires an H1 source summary and rejects a first description sentence that merely repeats it. Enable the standalone check with `"changesets/summary-description" = "error"` when not using the preset.
