---
"monochange": minor
"monochange_core": major
"monochange_config": minor
"monochange_schema": patch
---

# Add per-package and per-group bump ceilings and classification enforcement flags

Repositories can now set versioning policy per package instead of per repository. A prose-only package such as an agent skill can ship wording changes without a semantic-versioning gate demanding a minor or major bump, while crates that expose real APIs keep the gate.

- `bump_ceiling` clamps the classified proposed bump, enforceable minimum, and release floor for a package or group, and never raises a smaller bump.
- `classification_enforced = false` makes classification advisory for a package or group: the proposal still appears in reports, but the changeset-policy API gate never fails for it.
- Both fields resolve most-specific-first: a package declaration overrides its group's declaration, and a group declaration applies to members that do not declare their own. Grouped packages can therefore opt out while the rest of their group stays enforced, and a group can set one policy for every member.
- `PackageDefinition` and `GroupDefinition` expose the unset state as `Option`, so consumers constructing those structs pass `Some(..)` for an explicit declaration and `None` to inherit the group default or the built-in `true`. `EffectiveReleaseIdentity` keeps carrying the resolved value.

```toml
[group.main]
packages = ["cli", "docs-site"]

[package.docs-site]
path = "packages/docs-site"
type = "npm"
# Advisory only, and never propose more than a patch for prose.
bump_ceiling = "patch"
classification_enforced = false
```
