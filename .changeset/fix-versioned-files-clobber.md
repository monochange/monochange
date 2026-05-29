---
monochange: patch
---

# Fix versioned_files clobbering version field

Prevent `versioned_files` from updating the `version` field in native manifests unless explicitly specified in the `fields` configuration. This applies to all ecosystems: Cargo, Dart, Deno, npm, Python, and Go.

Previously, the version field would be overwritten whenever a group had `versioned_files` listed, even without `version` in the `fields` array.
