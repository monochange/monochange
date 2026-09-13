---
"monochange": minor
"monochange_core": major
"monochange_config": minor
"monochange_schema": patch
---

# Add per-package and per-group bump ceilings and classification enforcement flags

- `bump_ceiling` clamps the classified proposed bump, enforceable minimum, and release floor for a package or group.
- `classification_enforced = false` makes classification advisory for a package or group: the changeset-policy API gate never fails for it.
- Both fields are group-aware via effective release identity (groups override member packages).

```toml
[package.actions]
path = "."
type = "github_actions"
version_source = "tag"
initial_version = "0.1.0"
version_format = "primary"
```
