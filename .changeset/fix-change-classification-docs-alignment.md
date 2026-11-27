---
monochange_app: fix
---

# Align change classification reference documentation with bump alignment behavior

Updated `docs/src/reference/change-classification.md` so the changeset-policy bump-alignment section matches the implementation: changesets with `bump: none` (including types whose default bump is `none`) are still checked against API classification and fail if they understate impact, while only explicit-version-only changesets skip bump alignment. Also documented the Rust syntax analyzer's inert-attribute stripping, compatible attribute handling, and defaulted trait-item classification.
