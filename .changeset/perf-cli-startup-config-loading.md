---
"monochange": patch
"monochange_config": patch
---

# Speed up CLI startup

Version and help paths now avoid full workspace validation, and inherited versioned-file globs are deduplicated during package validation.
