---
"monochange": minor
"monochange_snapshot": minor
"@monochange/skill": patch
---

# Add normalized CLI snapshots

Add a `monochange_snapshot` crate for normalized command-surface snapshots and expose `mc snapshot` plus the global `--snapshot` flag. The snapshot output gives agents and CI a structured view of supported commands, options, arguments, standard entrypoints, and extractor provenance.
