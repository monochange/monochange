---
"monochange": patch
"monochange_config": patch
---

# Speed up CLI startup

Version and help paths now avoid full workspace validation. Full config loading now deduplicates inherited versioned-file glob validation and resolves glob checks against one ignored-file-aware workspace walk instead of repeatedly scanning the filesystem.
