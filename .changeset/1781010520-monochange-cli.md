---
"@monochange/skill": patch
monochange: patch
monochange_cli: patch
monochange_test_helpers: patch
---

# Extract CLI implementation into `monochange_cli`

Move the CLI implementation into a dedicated `monochange_cli` crate and reduce the published `monochange` crate to a tiny binary/facade that delegates to it. This keeps the installable command name stable while making the top-level package a thin shim around the reusable CLI implementation.
