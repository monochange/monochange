# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

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
