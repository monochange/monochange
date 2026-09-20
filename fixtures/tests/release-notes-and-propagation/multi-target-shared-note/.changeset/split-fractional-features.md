---
core:
  bump: major
  type: breaking
app:
  bump: minor
  type: feat
cli:
  bump: none
  type: docs
---

#### split floats and fixed, and source pods from pinapod

The fractional-field support is now two independent features.

This one changeset targets three packages with three different change types.
It must render once, in the breaking section, listing all three packages.
