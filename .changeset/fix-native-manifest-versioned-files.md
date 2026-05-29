---
monochange: patch
---

# Preserve native manifest updates before versioned_files

Apply `versioned_files` changes on top of native manifest updates so dependency constraints can be rewritten without clobbering package version fields.
