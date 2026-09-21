---
"@monochange/skill": patch
---

# Add `publish` to the skill's generated command inventory

The generated command inventory in `commands.md` now lists the built-in `publish` command, so an assistant following the skill sees `monochange publish packages`, `monochange publish readiness`, and `monochange publish placeholder` alongside the existing `monochange step *` entries.

The `next` and `next-versions` commands are aliases rather than distinct clap command literals, so they follow the same rule as the other top-level step aliases and stay out of this literal inventory.
