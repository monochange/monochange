---
core:
  bump: minor
  type: feat
cli:
  bump: minor
  type: fix
shared:
  bump: patch
---

#### Convert redundant changeset entries to inline form

Object entries that only repeat what the inline form already implies are
condensed to the inline form by `monochange check --fix`.
