---
monochange: minor
monochange_config: minor
monochange_graph: minor
monochange_semver: minor
monochange_core: minor
monochange_schema: docs
"@monochange/skill": docs
---

# declare how packages and groups propagate bumps to dependents

Release planning now supports per-package and per-group `bump_propagation` in `monochange.toml`. A package or group can declare that its changes are matched by dependents (`inherit`), bounded by a maximum (`bump_propagation_max`), pinned to a fixed floor (`none`/`patch`/`minor`/`major`), or left at the workspace `[defaults].parent_bump`. `monochange check` and release planning honor these declarations, and the JSON schema and the monochange skill documentation describe the new fields.

**Before (only the workspace-wide floor existed; a breaking dependency left dependents at `parent_bump`):**

```toml
[defaults]
parent_bump = "patch"
```

```markdown
---
sdk-core: breaking
---
```

Plan: `sdk-core` → major, but every dependent (app, cli) only → patch, even though a breaking dependency is itself breaking for them.

**After (declare inheritance with a clamp, and a floor on another package):**

```toml
[package."@solana/kit"]
path = "crates/kit"
bump_propagation = "inherit"
bump_propagation_max = "minor"

[package."@solana/leaf"]
path = "crates/leaf"
bump_propagation = "none"
```

```markdown
---
kit: breaking
---
```

Release plan: `kit` → major, `app` → minor (inherit matches breaking, clamped to minor), and nothing releases for the leaf's dependent. Groups can declare their own propagation, which overrides declarations of member packages, and changesets can still author an explicit bump for a dependent with `caused_by`.
