---
"monochange": minor
"monochange_core": major
monochange_schema: major
---

# Name provider releases after their owner and replay titles from release records

Provider releases (GitHub, GitLab, Gitea, Forgejo) published from a committed release record fell back to the bare tag name (`v0.22.0`) for the release title, because the record carried no rendered title and the manifest built from it blanked `rendered_title`. Release records now persist the title rendered at prepare time, and `build_release_manifest_from_record` replays it; records from schema v0.8 and earlier synthesize the built-in default title from the target id, version, and `created_at` date instead of degrading to the tag name.

The built-in release title template changes from `{{ version }} ({{ date }})` (primary) and `{{ id }} {{ version }} ({{ date }})` (namespaced) to a single default of `{{ id }} v{{ version }} ({{ date }})` for every version format, so a release attached to its tag names the package or group it belongs to. Repositories that prefer the old shape can restore it explicitly:

```toml
[defaults]
release_title = "{{ version }} ({{ date }})"
```

`ReleaseRecordTarget` gains `rendered_title` and `rendered_changelog_title` (optional, empty-string defaults), which is a breaking change for struct literals; deserialize and serialize round-trips of existing records are unchanged. The release-record artifact schema advances to v0.9 with a no-op migration edge, so v0.8 records migrate unchanged. `monochange_core::DEFAULT_RELEASE_TITLE_PRIMARY` and `DEFAULT_RELEASE_TITLE_NAMESPACED` now alias the new shared `monochange_core::DEFAULT_RELEASE_TITLE` constant and render with a `v`-prefixed version.
