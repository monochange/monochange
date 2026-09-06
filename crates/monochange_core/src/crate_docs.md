# `monochange_core`

<!-- {=monochangeCoreCrateDocs} -->

`monochange_core` is the shared vocabulary for the `monochange` workspace.

Reach for this crate when you are building ecosystem adapters, release planners, or custom automation and need one set of types for packages, dependency edges, version groups, change signals, and release plans.

## Why use it?

- avoid redefining package and release domain models in each crate
- share one error and result surface across discovery, planning, and command layers
- pass normalized workspace data between adapters and planners without extra translation

## Best for

- implementing new ecosystem adapters against the shared `EcosystemAdapter` contract
- moving normalized package or release data between crates without custom conversion code
- depending on the workspace domain model without pulling in discovery or planning behavior

## What it provides

- normalized package and dependency records
- version-group definitions and planned group outcomes
- change signals and compatibility assessments
- changelog formats, changelog targets, structured release-note types, release-manifest types, source-automation config types, and changeset-policy evaluation types
- shared error and result types

## Example

```rust
use monochange_core::render_release_notes;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogStyle;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;

let notes = ReleaseNotesDocument {
    title: "1.2.3".to_string(),
    summary: vec!["Grouped release for `sdk`.".to_string()],
    sections: vec![ReleaseNotesSection {
        title: "Features".to_string(),
        entries: vec!["- add keep-a-changelog output".to_string()],
        collapsed: false,
    }],
};

let rendered = render_release_notes(
    ChangelogFormat::KeepAChangelog,
    &notes,
    &ChangelogStyle::default(),
);

assert!(rendered.contains("## 1.2.3"));
assert!(rendered.contains("### Features"));
assert!(rendered.contains("- add keep-a-changelog output"));
```

<!-- {/monochangeCoreCrateDocs} -->
