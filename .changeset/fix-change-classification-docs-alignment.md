---
monochange_app: fix
---

# Align change classification reference documentation with bump alignment behavior

Updated `docs/src/reference/change-classification.md` so the changeset-policy bump-alignment section matches the implementation: every attached changeset target that resolves a bump (any scalar type, including types defaulting to `none`, or any table declaring `bump` or `type`, even alongside `version`) is checked against the package's classified recommendation, while only explicit-version targets omitting both `bump` and `type` skip alignment. Also documented the Rust syntax analyzer's inert-attribute stripping, compatible attribute handling, and dyn-compatibility-aware defaulted trait-item classification.
