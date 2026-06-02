---
"monochange": patch
"monochange_core": major
---

# Validate API-aligned changeset bumps

Affected changeset checks now compare requested release bumps with API classification output. Understated changesets fail so CI can catch risky releases, while overstated changesets report warnings for maintainers to inspect.

```json
{
	"requested": "patch",
	"recommended": "major",
	"status": "failed"
}
```
