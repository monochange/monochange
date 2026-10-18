---
monochange: fix
---

# Keep same-second changeset creations from overwriting each other

`monochange create` named changesets `<unix seconds>-<package>.md`, so two creations inside the same second (scripts, agents, or a quick second command) produced the same file and the second silently replaced the first. Both calls reported success while the first changeset was lost.

Default changeset names now append a counter when the timestamped name is already taken, so every creation gets its own file without passing `--output`.
