---
"monochange": minor
"monochange_changelog": patch
"monochange_config": minor
"monochange_core": minor
"monochange_graph": minor
"monochange_publish": patch
"monochange_schema": patch
"monochange_snapshot": major
---

# Add group package max bump controls

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
