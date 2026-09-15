---
monochange_changelog: minor
monochange_core: major
monochange_schema: minor
---

# Render each changeset once with every affected package

One changeset can list several targets, and each target carries its own change type. Because every target shares the changeset body, a file that targeted one package as `breaking`, another as `feat`, and a third as `docs` previously wrote the same paragraph into all three sections. Each copy also listed only the packages that happened to route to that section, so no copy showed the whole change.

`ReleaseNotesEntry.packages` is now `Vec<ReleaseNotePackage>` instead of `Vec<String>`. Each value pairs a package name with the `BumpSeverity` that package received, so a merged entry can report which package was major and which was minor. Construct the list with `ReleaseNotePackage::new(name, bump)`.

```rust
use monochange_core::BumpSeverity;
use monochange_core::ReleaseNotePackage;

let packages = vec![
	ReleaseNotePackage::new("core", BumpSeverity::Major),
	ReleaseNotePackage::new("cli", BumpSeverity::None),
];
```

`ChangelogStyle` and `ReleaseNotesStyleOverrides` gain a `package_bump_symbols` field, so struct literals must add it.

The section builder now merges entries that share a source changeset, summary, and details, keeps the entry in the configured section with the lowest `[changelog.sections.<id>].priority`, and appends every package to it. A change routed to a section above `[changelog.section_thresholds].ignored` still contributes its packages instead of disappearing. Entries without a source path are synthesized empty-update messages and are never merged, because two packages legitimately produce similar text.

Package labels are prefixed with `🔴` major, `🟠` minor, `🟢` patch, or `⚪` none. `ChangelogStyle::rules()` reports the active setting.

```toml
[changelog.style]
package_bump_symbols = false
```

The committed `monochange.schema.json` gains the `package_bump_symbols` and `packages` definitions. The durable `ReleaseNotesDocument<String>` artifact shape is unchanged, so providers that read release records and compare rendered entries keep working.
