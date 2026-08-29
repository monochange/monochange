---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_semver: minor
monochange_graph: minor
monochange_schema: docs
"@monochange/skill": docs
---

# add `[defaults].bump_propagation` and most-specific-first precedence

Bump propagation policies now resolve most-specific-first: a package declaration overrides its group declaration, which overrides the new `[defaults].bump_propagation` (with the optional `[defaults].bump_propagation_max` clamp), which overrides the legacy `[defaults].parent_bump` floor.

**Before (only the workspace floor applied to undeclared targets):**

```toml
[defaults]
parent_bump = "major"
```

Every dependent of any changed package had to release major, even when the source's change was a patch.

**After (workspace-wide inherit fallback without redeclaring per package):**

```toml
[defaults]
bump_propagation = "inherit"

[group.kit]
packages = ["kit-core"]

[package.kit-core]
bump_propagation = "inherit"
bump_propagation_max = "minor"
```

Precedence: the most specific declaration wins — the kit-core package clamp (minor) overrides the group's unclamped inherit, which overrides the defaults. Packages and groups with no declaration pick up the defaults inherit; a target can still pin itself with `bump_propagation = "none"`.
