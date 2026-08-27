---
monochange: feat
monochange_config: feat
---

# condense redundant changeset entries with `changesets/prefer-inline`

Change entries written as objects can now be flagged and automatically rewritten when they only repeat what the inline form already implies. `monochange check` reports a `changesets/prefer-inline` error for entries such as `bump` plus `type` where the type already implies the same bump, and `monochange check --fix` collapses them to the concise inline entry. The rule is on by default for every project and is part of the `changesets/recommended` preset.

**Before:**

```markdown
---
"@monochange/cli":
  bump: minor
  type: feat
---
```

**After (`monochange check --fix`):**

```markdown
---
"@monochange/cli": feat
---
```

Entries that would change meaning inline are left untouched: explicit `version` values, `caused_by` references, bumps that disagree with the type default, and bare `bump` entries that would gain a change type.
