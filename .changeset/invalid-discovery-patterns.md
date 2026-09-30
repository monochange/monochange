---
monochange_config: patch
---

# Diagnose invalid automatic-discovery patterns

Malformed include or exclude globs now fail configuration loading with the ecosystem, field, and offending pattern. Previously an invalid pattern could silently hide packages or misalign the remaining include patterns. Excludes are validated even when the include list is empty.

```toml
[ecosystems.cargo.auto_discover]
include = ["crates/["]
```

This configuration now produces an actionable error instead of an empty package inventory. Correct the pattern, for example to `crates/*`, before retrying.
