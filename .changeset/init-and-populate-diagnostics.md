---
monochange: patch
---

# Explain unsupported initializer ownership and workflow defaults

`monochange init` rejects multiple discovered package owners in one directory before writing configuration or provider workflows. Previously it could generate a starter configuration that immediately failed validation. Separate those packages into different directories or configure one release owner manually.

When packages in separate directories share a name, generated package IDs now remain unique even for three or more owners and avoid colliding with another package's native name. Existing available IDs are preserved; extra owners receive deterministic numeric suffixes.

`monochange populate` continues to preserve existing configuration. Its help and output now explain that this version provides no built-in CLI workflow defaults, instead of implying that workflow aliases were added. Define custom workflows under `[cli.<name>]`, or use the built-in `create`, `preview`, and `prepare` commands directly.
