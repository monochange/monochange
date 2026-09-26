---
"monochange_core": major
"monochange_changelog": major
"monochange_hosting": major
"monochange_github": major
"monochange_config": major
monochange_schema: major
---

# Merge grouped release notes into one deduplicated change list

Grouped releases rendered one section per member package, so a change shared by several packages was printed once per package under a repeated `## Features` heading. A group release now lists every change once, in its configured section, with the packages it affects named in the entry itself.

The `[changelog.style].package_label_placement` setting and its `release_notes` override are removed. Packages are always metadata about a change, rendered as a `_Packages:_` line directly above the `_Owner:_` line, so the single-package inline form (`- 🟠 **pkg**: summary`) and the `after_heading`/`after_change` values no longer exist. Remove the key from `monochange.toml`; the strict config parser rejects the unknown field.

```toml
[changelog.style]
# Remove this key; package labels are always rendered above the owner line.
# package_label_placement = "after_heading"
```

Affected packages keep their per-package bump symbols, so a merged entry still shows which package was major and which was minor:

```markdown
## Fixes

- **Fix shared bug.** _Packages:_ 🟠 _core_, 🟢 _cli_ _Owner:_ @ifiokjr · _Review:_ [PR #725](https://github.com/monochange/monochange/pull/725)
```

The group `include` filter is unchanged for changelog files: `include = ["app"]` still curates the committed changelog. A provider release body is no longer derived from that filtered file, so a filter that hides internal notes from a changelog cannot publish a release that claims nothing happened. The `uncovered_member_changelogs`, `grouped_member_release_body`, and `push_member_changelogs` helpers are deleted from `monochange_hosting` and their duplicate in `monochange_github`.

The published configuration contract changed, so the schemas advance to `v0.8`; the `0.7` → `0.8` migration edge accepts existing release records unchanged.
