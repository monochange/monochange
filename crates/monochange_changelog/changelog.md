# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.17.1](https://github.com/monochange/monochange/releases/tag/v0.17.1) (2026-10-07)

### Changed

- **No package-specific changes were recorded; `monochange_changelog` was updated to 0.17.1 as part of group `main`.**

## [0.17.0](https://github.com/monochange/monochange/releases/tag/v0.17.0) (2026-10-06)

### 🐛 Fixed

#### Insert new releases above namespaced changelog headings

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #746](https://github.com/monochange/monochange/pull/746)

Changelogs owned by a package or group with `version_format = "namespaced"` collected their releases oldest first. The built-in namespaced changelog version title leads with the owner id (`## abi [0.20.0](…) (2026-09-23)`), but monochange only recognised an earlier release when its `##` heading started with `[` or a digit. With no recognised heading, every prepared release was appended to the end of the file, while `primary` owners stayed newest first. A custom `changelog_version_title` that started with text, such as `"SDK {{ version }} ({{ date }})"`, had the same problem.

monochange now treats any `##` heading that contains a semantic version as a release heading, whatever text surrounds the version, and inserts the new section directly above the first one. For a namespaced group changelog, `monochange step prepare-release` now writes:

**Before:**

```markdown
## abi [0.20.0](https://github.com/acme/repo/releases/tag/abi/v0.20.0) (2026-09-23)

## abi [0.21.0](https://github.com/acme/repo/releases/tag/abi/v0.21.0) (2026-09-30)
```

**After:**

```markdown
## abi [0.21.0](https://github.com/acme/repo/releases/tag/abi/v0.21.0) (2026-09-30)

## abi [0.20.0](https://github.com/acme/repo/releases/tag/abi/v0.20.0) (2026-09-23)
```

The rendered section is unchanged; only where it lands in the file moves. Hand-written headings without a version, including `## Unreleased` and Keep a Changelog's `## [Unreleased]`, now stay above the new release instead of being treated as releases. A custom title must render `{{ version }}` for monochange to find earlier releases, so a date-only title is no longer recognised.

Existing changelogs that already collected releases oldest first are not reordered. Move those sections once by hand; later releases then land at the top. Workarounds that switched a namespaced owner to a title starting with `[`, such as `changelog_version_title = "[{{ version }}]({{ tag_url }}) ({{ date }})"`, can be removed.

#### Use the coloured monochange identity in documentation

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #749](https://github.com/monochange/monochange/pull/749)

The README, crate documentation, and guide now use the selected flowing monochange mark in violet and indigo, with matching light and dark wordmarks. Square mark-only images keep the identity readable in organization avatars, Rust documentation navigation, and browser tabs.

Existing Rust documentation image URLs remain valid and receive the new artwork when this change reaches the default branch. No API, configuration, or release behavior changes.

## [0.16.0](https://github.com/monochange/monochange/releases/tag/v0.16.0) (2026-09-30)

### Changed

#### No package-specific changes were recorded; `monochange_changelog` was updated to 0.16.0 as part of group `main`.

## [0.15.0](https://github.com/monochange/monochange/releases/tag/v0.15.0) (2026-09-28)

### 💥 Breaking Change

#### Merge grouped release notes into one deduplicated change list

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #725](https://github.com/monochange/monochange/pull/725) · _Related issues:_ [#725](https://github.com/monochange/monochange/issues/725)

Grouped releases rendered one section per member package, so a change shared by several packages was printed once per package under a repeated `## Features` heading. A group release now lists every change once, in its configured section, with the packages it affects named in the entry itself.

The `[changelog.style].package_label_placement` setting and its `release_notes` override are removed. Packages are always metadata about a change, rendered as a `_Packages:_` line directly above the `_Owner:_` line, so the single-package inline form (`- 🟠 **pkg**: summary`) and the `after_heading`/`after_change` values no longer exist. Remove the key from `monochange.toml`; the strict config parser rejects the unknown field.

```toml
[changelog.style]
# Remove this key; package labels are always rendered above the owner line.
# package_label_placement = "after_heading"
```

Affected packages keep their per-package bump symbols, so a merged entry still shows which package was major and which was minor. A compact entry keeps the package line beside the bullet text:

```markdown
## Fixes

- **Fix shared bug.** _Packages:_ 🟠 _core_, 🟢 _cli_ _Owner:_ @ifiokjr · _Review:_ [PR #725](https://github.com/monochange/monochange/pull/725)
```

An expanded entry — a breaking change, or any change whose body has a code block or several paragraphs — renders the package line directly beneath its heading, above the explanation:

```markdown
### Split the release note renderer

_Packages:_ 🔴 _core_, 🟠 _app_, 🟢 _cli_ _Owner:_ @ifiokjr · _Review:_ [PR #725](https://github.com/monochange/monochange/pull/725)

One changeset targets three packages with three different change types, so the group release publishes it once in the breaking section.
```

The group `include` filter is unchanged for changelog files: `include = ["app"]` still curates the committed changelog. A provider release body is no longer derived from that filtered file, so a filter that hides internal notes from a changelog cannot publish a release that claims nothing happened. The `uncovered_member_changelogs`, `grouped_member_release_body`, and `push_member_changelogs` helpers are deleted from `monochange_hosting` and their duplicate in `monochange_github`.

The published configuration contract changed, so the schemas advance to `v0.8`; the `0.7` → `0.8` migration edge accepts existing release records unchanged.

## [0.14.0](https://github.com/monochange/monochange/releases/tag/v0.14.0) (2026-09-19)

### 🚀 Feature

#### Render each changeset once with every affected package

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #708](https://github.com/monochange/monochange/pull/708)

### 🐛 Fixed

#### Skip change classification on release pull requests and rename the `unknown` impact

- New crate `monochange_classification` owns the classification report contract and its schema, versioned independently of the release train.

- `monochange change classify` accepts `--label` and reads `[changesets.classification].skip_labels` (default `["release"]`). A matching label reports `skipped: true`, analyzes no packages, and exits successfully, so the release pull request monochange opens is no longer classified.
- The `unknown` compatibility impact is now `unmodeled`. The change is still outside the analyzer's modeled public surface, but the package itself is supported, so the previous name overstated how much was unknown.
- The `change-classification` GitHub Action gained a `labels` input and defaults it to the current pull request's labels. A skipped run deletes any comment left from an earlier revision.

```toml
[changesets.classification]
# Set to [] to classify every pull request.
skip_labels = ["release"]
```

```bash
monochange change classify --format json --label release
```

The published configuration contract gained `[changesets.classification]`, so the schemas advance to `v0.7`; the `0.6` → `0.7` migration edge accepts existing release records unchanged.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #709](https://github.com/monochange/monochange/pull/709)

## [0.13.0](https://github.com/monochange/monochange/releases/tag/v0.13.0) (2026-09-13)

### Changed

#### No package-specific changes were recorded; `monochange_changelog` was updated to 0.13.0 as part of group `main`.

## [0.12.0](https://github.com/monochange/monochange/releases/tag/v0.12.0) (2026-09-12)

### Changed

#### No package-specific changes were recorded; `monochange_changelog` was updated to 0.12.0 as part of group `main`.

## [0.11.1](https://github.com/monochange/monochange/releases/tag/v0.11.1) (2026-09-10)

### Changed

- **No package-specific changes were recorded; `monochange_changelog` was updated to 0.11.1 as part of group `main`.**

## [0.11.0](https://github.com/monochange/monochange/releases/tag/v0.11.0) (2026-09-09)

### 🚀 Feature

#### Keep release notes structured until their output format is known

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #668](https://github.com/monochange/monochange/pull/668)

## [0.10.0](https://github.com/monochange/monochange/releases/tag/v0.10.0) (2026-09-03)

### 💥 Breaking Change

#### render named changelog outputs from one audience stream

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #652](https://github.com/monochange/monochange/pull/652)

## [0.9.2](https://github.com/monochange/monochange/releases/tag/v0.9.2) (2026-08-29)

<details>
<summary><strong>📖 Documentation</strong></summary>

#### add the monochange logo across readme, docs.rs, and the mdBook

Every published crate now renders the monochange mark on docs.rs through `html_logo_url`, and docs.rs pages use the matching favicon through `html_favicon_url`. The mark itself is a chunky lowercase `mc` monogram with a version-bump arrow in the negative space.

- the readme gains a top-level hero logo that follows the reader's theme: a light variant on light GitHub themes and a light-on-dark variant on dark themes, using the `picture` element with `prefers-color-scheme`
- the mdBook in `docs/` picks up a new `favicon.png`
- `assets/` holds the exported logo sizes (280, 512, 1024), the dark variant, and a multi-size `favicon.ico`
- a reserve mark (the navy Converge badge) is kept under `assets/reserve/` for a future rebrand

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #641](https://github.com/monochange/monochange/pull/641)

</details>

## [0.9.1](https://github.com/monochange/monochange/releases/tag/v0.9.1) (2026-08-19)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.9.1 as part of group `main`.

## [0.9.0](https://github.com/monochange/monochange/releases/tag/v0.9.0) (2026-08-14)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.9.0 as part of group `main`.

## [0.8.4](https://github.com/monochange/monochange/releases/tag/v0.8.4) (2026-07-11)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.8.4 as part of group `main`.

## [0.8.3](https://github.com/monochange/monochange/releases/tag/v0.8.3) (2026-06-29)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.8.3 as part of group `main`.

## [0.8.2](https://github.com/monochange/monochange/releases/tag/v0.8.2) (2026-06-18)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.8.2 as part of group `main`.

## [0.8.1](https://github.com/monochange/monochange/releases/tag/v0.8.1) (2026-06-09)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.8.1 as part of group `main`.

## [0.8.0](https://github.com/monochange/monochange/releases/tag/v0.8.0) (2026-06-04)

### 🐛 Fixed

#### Add group package max bump controls

Allow version group package entries to use table syntax with `max_bump` so a member can cap how much its own changes raise the group version. String package entries keep the existing behavior and table entries default to `max_bump = "major"`; `max_bump = "none"` keeps the package aligned with the group without allowing that package's own changes to raise the group bump.

Rename CLI snapshot bump-cap fields from `max_semver_bump` to `max_bump`.

```json
{
	"commands": [
		{
			"path": ["experimental"],
			"max_bump": "minor"
		}
	]
}
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #602](https://github.com/monochange/monochange/pull/602)

## [0.7.0](https://github.com/monochange/monochange/releases/tag/v0.7.0) (2026-06-03)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.7.0 as part of group `main`.

## [0.6.8](https://github.com/monochange/monochange/releases/tag/v0.6.8) (2026-05-31)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.6.8 as part of group `main`.

## [0.6.7](https://github.com/monochange/monochange/releases/tag/v0.6.7) (2026-05-30)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.6.7 as part of group `main`.

## [0.6.6](https://github.com/monochange/monochange/releases/tag/v0.6.6) (2026-05-29)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.6.6 as part of group `main`.

## [0.6.5](https://github.com/monochange/monochange/releases/tag/v0.6.5) (2026-05-29)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.6.5 as part of group `main`.

## [0.6.4](https://github.com/monochange/monochange/releases/tag/v0.6.4) (2026-05-28)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.6.4 as part of group `main`.

## [0.6.3](https://github.com/monochange/monochange/releases/tag/v0.6.3) (2026-05-28)

### 🐛 Fixed

#### Filter group-propagated changes from per-package changelogs

When a package is a member of a version group, its per-package changelog now only includes changes from changesets that directly target that package (kind=Package), not changes propagated from group-level targeting (kind=Group). Group-level changes appear exclusively in the group changelog, eliminating content duplication across member changelogs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #549](https://github.com/monochange/monochange/pull/549) · _Closed issues:_ [#548](https://github.com/monochange/monochange/issues/548)

## [0.6.2](https://github.com/monochange/monochange/releases/tag/v0.6.2) (2026-05-27)

### 🚀 Feature

#### Add `Inline` metadata style and make it the default

Context blocks in changelog entries now render as a single inline paragraph joined with `·` instead of separate lines.

When a review request (PR/MR) link is available, commit links are omitted since the PR already identifies the change. When no review request link exists, commit links are included as before.

The existing `Plain` and `Blockquote` styles continue to render commit links unconditionally. The `Omit` style hides all metadata as before.

**Before (default: `plain`):**

```markdown
# Add release summary panel

_Owner:_ @user _Review:_ [PR #123](https://...) _Introduced in:_ [`abc1234`](https://...) _Related issues: #456
```

**After (default: `inline`):**

```markdown
# Add release summary panel

_Owner:_ @user · _Review:_ [PR #123](https://...) · _Related issues: #456
```

Set `metadata_style = "inline"` (now the default), `"plain"`, `"blockquote"`, or `"omit"` under `[changelog.style]` in `monochange.toml`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #532](https://github.com/monochange/monochange/pull/532) · _Related issues:_ [#123](https://github.com/monochange/monochange/issues/123), [#456](https://github.com/monochange/monochange/issues/456)

## [0.6.1](https://github.com/monochange/monochange/releases/tag/v0.6.1) (2026-05-24)

### Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.6.1 as part of group `main`.

## [0.6.0](https://github.com/monochange/monochange/releases/tag/v0.6.0) (2026-05-23)

### 🚀 Feature

#### Add configurable changelog rendering styles

Add configurable changelog and release-note rendering style options for section separators, package labels, metadata lines, and collapsed sections.

```toml
[changelog.style]
sectionSeparator = "blank_line"
packageLabelStyle = "inline"
packageLabelPlacement = "after_heading"
metadataStyle = "plain"
collapsedSectionStyle = "details"

[changelog.release_notes]
metadataStyle = "blockquote"
```

The config schema now includes `ChangelogStyle` and `ReleaseNotesStyleOverrides`, with release notes inheriting `[changelog.style]` unless a field-specific override is set.

Default section headings now include emoji in the `heading` string, while the stable section keys remain unchanged:

- `breaking`: `💥 Breaking Change`
- `feat`: `🚀 Feature`
- `change`: `📝 Changed`
- `fix`: `🐛 Fixed`
- `test`: `🧪 Testing`
- `refactor`: `🔨 Refactor`
- `docs`: `📖 Documentation`
- `security`: `🔒 Security`
- `perf`: `⚡ Performance`
- `none`: `🔖 None`

Semver level type aliases route to semantic sections: `major` to `breaking`, `minor` to `feat`, and `patch` to `fix`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #511](https://github.com/monochange/monochange/pull/511) _Introduced in:_ [`b03612b`](https://github.com/monochange/monochange/commit/b03612b5d69f05becd68a803efa535e0f874ee01) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

### 🐛 Fixed

#### Add prerelease mode

Add first-class prerelease configuration and release planning support.

Prerelease mode now writes `.monochange/prerelease-state.json`, preserves the original stable baseline across repeated prerelease preparations, supports planned/current/fixed stable bases, and can synthesize prerelease plans without changesets.

Validation now rejects stale prerelease state when prerelease mode is disabled, stable release preparation removes the prerelease state file, and `[prerelease].branches` can override stable release branch restrictions for prerelease tag/publish steps.

Enable incrementing alpha prereleases from the next planned stable version:

```toml
[prerelease]
enabled = true
channel = "alpha"
numbering = "increment"
base = "planned"
branches = ["next", "prerelease/*"]
```

Use release-candidate prereleases from the current stable baseline when you want a tagged binary build without applying changeset bump severity yet:

```toml
[prerelease]
enabled = true
channel = "rc"
numbering = "increment"
base = "current-stable"
publish_packages = false
```

Use a fixed `0.0.0` nightly-style prerelease line with date-based identifiers:

```toml
[prerelease]
enabled = true
channel = "nightly"
numbering = "date"
base = "fixed"
base_version = "0.0.0"
keep_changesets = true
changelog = false
release_notes = true
publish_packages = false
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #522](https://github.com/monochange/monochange/pull/522) _Introduced in:_ [`9a5fe30`](https://github.com/monochange/monochange/commit/9a5fe305600c17364f8916fe9cfc160825dfda5c) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 📝 Changed

- No package-specific changes were recorded; `monochange_changelog` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 🚀 Feature

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### 🚀 Feature

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### 🐛 Fixed

#### Split crate boundaries for changelog, config, and publish behavior

Move changelog rendering into `monochange_changelog`, shift publish planning and execution helpers into `monochange_publish`, and reduce direct concrete ecosystem/provider dependencies in `monochange_config`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #441](https://github.com/monochange/monochange/pull/441) _Introduced in:_ [`ae8ea56`](https://github.com/monochange/monochange/commit/ae8ea563ae95c6cc4e8d3d1acdc5303069ea44cf)
