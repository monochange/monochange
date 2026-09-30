---
monochange_cargo: patch
---

# Ignore documentation changes when classifying Rust API impact

Rust API analysis now ignores documentation attributes on public items and their nested fields, variants, and trait members. Editing, adding, or removing rustdoc no longer requests a major release. Type changes and API-affecting attributes still contribute to release classification.
