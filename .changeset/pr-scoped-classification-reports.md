---
"monochange": minor
"monochange_classification": major
"@monochange/skill": patch
---

# Keep classification reports specific to the pull request

- The report records the commits it compared in the new top-level `base_commit` and `head_commit` fields. Match `head_commit` with the pull request head to confirm a saved report is current; a changed `base_commit` shows the base branch moved since the report was built.
- Markdown and text reports list only findings the pull request produced under each package. A finding whose `comparisons` contain only `release` and `release_to_default` now appears under "Unreleased changes already on `<base>` (not part of this pull request)" and no longer counts toward the package's findings. The JSON `findings` array is unchanged, because the release floor still needs that evidence.
- `--base` is documented as the pull request's base branch. Pass it for a stacked pull request so the classifier does not attribute the parent branch's changes to the child:

```bash
monochange change classify --base origin/feature/parent --head "$PR_HEAD_SHA" --format json
```

The classification report contract advances to `schema_version` `0.4` with a frozen `classification.v0.4.schema.json`. The change is additive: readers of `0.3` reports keep working.
