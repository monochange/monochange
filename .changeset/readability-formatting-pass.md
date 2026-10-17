---
main: refactor
monochange_app: refactor
monochange_classification: refactor
monochange_schema: refactor
monochange_snapshot: refactor
---

# Readability formatting across the workspace

Apply monostyle's formatter-safe whitespace conventions and dprint normalization across every crate — blank-line separation around control flow and consistent formatting only, with no behavior changes. The repository now scores 84.5 overall on monostyle (readability 89.6) and gates future changes through `lint:monostyle`.
