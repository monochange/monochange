---
"@monochange/skill": patch
---

# Document how non-Rust CLIs emit command snapshots

The skill bundle now explains that the command snapshot document can be produced outside Rust. The configuration and change-classification skills point at the published command snapshot schema and the new emitter guide, and describe the values fixed by the contract (`kind` is always `"cli-surface"`; `schema_version` must match the supported snapshot contract version) plus the rule that a `failed` capture status usually means the emitted document does not match the schema.
