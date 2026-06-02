---
"monochange": patch
"monochange_cargo": patch
"monochange_dart": patch
"monochange_npm": patch
---

# Improve `mc check` onboarding defaults

Add per-ecosystem `baseline` lint presets and make generated `monochange.toml` files start with those softer presets. Baseline presets keep onboarding diagnostics as warnings or opt out of formatting-style checks so existing repositories can adopt `mc check` before escalating to `recommended` or `strict`.
