---
"monochange": patch
---

# Render prerelease release notes from the changes added since the previous prerelease

Prerelease mode discarded every changelog artifact, so `[prerelease].release_notes = true` had no effect and a hosted prerelease was published with an empty or minimal body. `prepare-release` now renders the configured release-note outputs for a prerelease while still leaving changelog files untouched unless `[prerelease].changelog = true`.

Because `keep_changesets = true` leaves earlier changeset files in place, each prerelease reports only the changesets **added since the previous prerelease**. A changeset already reported by an earlier prerelease in the same series is omitted, and editing the body of an already reported changeset does not make it reappear. The reported paths are recorded in `.monochange/prerelease-state.json`:

```json
{
	"schema_version": 1,
	"channel": "alpha",
	"release_note_changesets": [".changeset/first.md"]
}
```

Two events restart the series and present every pending change again: changing `[prerelease].channel`, and removing the state file. Set `release_notes = false` to suppress prerelease note artifacts entirely.

`PreparedRelease.updated_changelogs` now lists only the changelog files the release actually rewrites, so a prerelease that publishes notes without touching changelog files no longer reports them as updated.
