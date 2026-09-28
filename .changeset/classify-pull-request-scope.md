---
"monochange": minor
"monochange_cargo": minor
"monochange_classification": major
"@monochange/skill": patch
---

# Scope the classified bump to the pull request

- `decision.pull_request_changes` reports whether this contribution touches the package. When it is `false`, every finding came from the `release` or `releaseToDefault` interval, so `decision.proposed_changeset_bump` and `decision.enforceable_minimum` are `none`, `decision.review_required` is `false`, and the accumulated change appears only in `decision.release_floor` and `decision.release_impact`. The net candidate and the local working tree set the flag; the release comparisons never do.
- A pending changeset for a package the pull request does not modify still reports `action: review` with the summary `the pull request does not change this package; the pending changeset intent needs review`, because a changeset can intentionally describe a consumer-facing effect implemented in another package. It no longer escalates the bump or the review verdict for work an earlier merge introduced.
- The Cargo analyzer classifies a pure append to a public `const`/`static` whose declared type is `&[T]`, `[T; N]`, or `Vec<T>` and whose initializer is a literal as `additive`/`minor` with high confidence, matching `cargo semver-checks`. A removal, a reorder, an element edit, a changed element type, or a non-literal initializer stays conservative.

```json
{
	"compatibility_impact": "compatible",
	"release_impact": "breaking",
	"pull_request_changes": false,
	"proposed_changeset_bump": "none",
	"release_floor": "major",
	"review_required": false
}
```

The classification report contract advances to `schema_version` `0.3` (`SCHEMA_VERSION` regenerated with a frozen `classification.v0.3.schema.json`; the shipped v0.1 and v0.2 assets are untouched, and the schemas reference page lists the new asset). `decision.pull_request_changes` is new, and `decision.proposed_changeset_bump`, `decision.enforceable_minimum`, and `decision.review_required` can be lower for a package this pull request does not touch.

No configuration change is required. Re-run `monochange change classify` to pick up the pull-request-scoped verdict.
