---
"monochange": minor
"monochange_core": major
monochange_schema: major
"@monochange/skill": patch
---

# Name release-record replays and format-specific default release titles

Provider releases (GitHub, GitLab, Gitea, Forgejo) published from a committed release record fell back to the bare tag name (`v0.22.0`) for the release title, because the record carried no rendered title and the manifest built from it blanked `rendered_title`. Release records now persist the title rendered at prepare time, and `build_release_manifest_from_record` replays it; records from schema v0.8 and earlier synthesize the built-in default title for the target's version format, dated from the record's `created_at`, instead of degrading to the tag name.

The built-in release title defaults are now format-specific:

- primary versioning renders `v{{ version }} ({{ date }})` — one release axis, so the title carries the tag-style version with the date;
- namespaced versioning renders `{{ id }} v{{ version }} ({{ date }})` — the owner is named because a workspace releases several axes at once.

Repositories that prefer another shape can set it explicitly on a package, a group, or workspace-wide:

```toml
[defaults]
release_title = "{{ id }} {{ version }} ({{ date }})"
```

`ReleaseRecordTarget` gains `rendered_title` and `rendered_changelog_title` (optional, empty-string defaults), which is a breaking change for struct literals; deserialize and serialize round-trips of existing records are unchanged. The release-record artifact schema advances to v0.9 with a no-op migration edge, so v0.8 records migrate unchanged.

The agent skill's configuration topic now documents the release title templates — the defaults per version format, the available variables, precedence, and the record replay — and `@monochange/skill` republishes that guidance.
