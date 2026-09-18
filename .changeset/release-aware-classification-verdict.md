---
"monochange": minor
"monochange_classification": major
"@monochange/skill": patch
---

# Report a break against main separately from the release verdict

- `decision.release_impact` reports the compatibility impact measured between the package's latest release and the candidate. A pull request that only changes an API the latest release never contained now reads `compatibility_impact: breaking` with `release_impact: additive`.
- `decision.proposed_changeset_bump` and `decision.enforceable_minimum` are capped by the release comparison, so a modeled finding can no longer propose a bump higher than the release-relative bump for the same package. Nobody holding the latest release can observe a break in an item that the release comparison does not show as changed.
- Unmodeled findings stay uncapped. They are the safety floor for a surface the analyzers cannot model, and the release comparison cannot refute them.
- `decision.release_floor` reports the accumulated unreleased bump without inheriting a break that only exists against the default branch.
- The classification report contract advances to `schema_version` `0.2`: `decision.release_impact` is new, and `decision.proposed_changeset_bump`, `decision.enforceable_minimum`, and `decision.release_floor` can be lower than in `0.1` for the same pull request.

```json
{
	"compatibility_impact": "breaking",
	"release_impact": "additive",
	"proposed_changeset_bump": "minor",
	"release_floor": "minor"
}
```

No configuration change is required. Re-run `monochange change classify` to pick up the release-relative verdict.
