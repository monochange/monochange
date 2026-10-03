---
monochange_changelog: fix
---

# Insert new releases above namespaced changelog headings

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
