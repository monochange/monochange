# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.6.0](https://github.com/monochange/monochange/releases/tag/v0.6.0) (2026-05-22)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #511](https://github.com/monochange/monochange/pull/511) _Introduced in:_ [`b03612b`](https://github.com/monochange/monochange/commit/b03612b5d69f05becd68a803efa535e0f874ee01)

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

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #522](https://github.com/monochange/monochange/pull/522) _Introduced in:_ [`9a5fe30`](https://github.com/monochange/monochange/commit/9a5fe305600c17364f8916fe9cfc160825dfda5c)

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
