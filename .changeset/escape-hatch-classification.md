---
"monochange": minor
"monochange_core": minor
"monochange_config": minor
---

# Add per-package and per-group bump ceilings and classification enforcement flags

- `bump_ceiling` clamps the classified proposed bump, enforceable minimum, and release floor for a package or group.
- `classification_enforced = false` makes classification advisory for a package or group: the changeset-policy API gate never fails for it.
- Both fields are group-aware via effective release identity (groups override member packages).
