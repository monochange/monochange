---
monochange_app: patch
---

# Patch pulldown-cmark exponential parsing advisory

Bump `pulldown-cmark` from 0.13.4 to 0.13.6 in `app/Cargo.lock`.

0.13.4 grows its event stream exponentially when `ENABLE_WIKILINKS` is set, so a crafted markdown document pins a CPU for the duration of the parse (RUSTSEC-2026-0414, RUSTSEC-2026-0433). Patch-level move inside the existing 0.13 requirement; no source changes.
