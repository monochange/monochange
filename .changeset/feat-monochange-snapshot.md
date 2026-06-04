---
"monochange": minor
"monochange_snapshot": major
"@monochange/skill": patch
---

# Add normalized CLI snapshots

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
