---
monochange: minor
monochange_semver: minor
"@monochange/skill": patch
---

# add semantic semver guardrails to release planning

Release planning now folds semantic analyzer evidence into compatibility assessments so public API and export diffs can raise the planned bump during previews. The guardrail is advisory: analyzer failures and uncovered semantic changes are reported as warnings instead of blocking release planning.
