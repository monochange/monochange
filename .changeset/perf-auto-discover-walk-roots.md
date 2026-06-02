---
"monochange_config": patch
---

# Speed up ecosystem auto-discovery

Limit auto-discovery filesystem walks to the literal path prefix of each `auto_discover.include` glob instead of walking the repository root for every ecosystem pattern.
