---
monochange: feat
monochange_config: feat
---

# condense redundant changeset entries with `changesets/prefer-inline`

Change entries written as objects can now be flagged and automatically rewritten when they only repeat what the inline form already implies. `monochange check` reports a `changesets/prefer-inline` error for entries such as `bump` plus `type` where the type already implies the same bump, for objects that only declare `type`, and for bare `bump` entries whose bump keyword is also a change type that implies the same bump. `monochange check --fix` collapses them to the concise inline entry. The rule is on by default for every project and is part of the `changesets/recommended` preset.

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

A bare bump entry is converted too when the bump keyword is also a change type that implies the same bump: `"@monochange/cli": { bump: minor }` becomes `"@monochange/cli": minor`, keeping the bump and gaining the type.

Entries that would change meaning inline are left untouched: explicit `version` values, `caused_by` references, bumps that disagree with the type default, and bare `bump` keywords that are not configured change types.
