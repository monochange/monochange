---
"monochange": patch
"monochange_lint": patch
"monochange_npm": patch
---

# Improve `mc check` lint diagnostics

Show lint rule IDs first in check output, add `--verbose` details, derive line and column locations from lint spans, avoid running regular package lint rules against unmanaged packages, and keep npm workspace-protocol checks opt-in by default.
