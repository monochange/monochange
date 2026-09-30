---
"@monochange/skill": patch
---

# Document the reorganized monochange command line

The skill's command reference now explains that `[cli.<name>]` workflows also run as `monochange <name>` (with `monochange run <name>` still preferred in scripts), that bare `monochange versions` is a read-only check, the global `--verbose` flag, and that usage errors exit with status `2` while `--progress-format json` reports failures as a `diagnostic` event.
