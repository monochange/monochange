---
core:
  bump: major
  type: breaking
app:
  bump: minor
  type: feat
cli:
  bump: patch
  type: fix
---

# Split the release note renderer

One changeset targets three packages with three different change types, so the
group release must publish it once in the breaking section with every affected
package listed.
