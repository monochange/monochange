# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## snapshot [0.1.0](https://github.com/monochange/monochange/releases/tag/snapshot/v0.1.0) (2026-06-04)

Grouped release for `snapshot`.

### 💥 Breaking Change

#### Add group package max bump controls

_Packages:_ _monochange_snapshot_

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

#### Add normalized CLI snapshots

_Packages:_ _monochange_snapshot_

Add a `monochange_snapshot` crate for normalized command-surface snapshots and expose `mc snapshot` plus the global `--snapshot` flag. The snapshot output gives agents and CI a structured view of supported commands, options, arguments, standard entrypoints, and extractor provenance.

For example, a CLI can produce a normalized snapshot with a stable schema version and extractor provenance:

```json
{
	"schema_version": "0.1",
	"kind": "cli-surface",
	"tool": {
		"name": "mc",
		"version": "0.7.0"
	},
	"provenance": {
		"extractor": "clap",
		"confidence": "high"
	},
	"commands": [
		{
			"path": ["snapshot"],
			"max_bump": "major",
			"hidden": false
		}
	]
}
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #593](https://github.com/monochange/monochange/pull/593)
