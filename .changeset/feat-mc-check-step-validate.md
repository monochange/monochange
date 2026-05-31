---
monochange: minor
---

# Add `step:validate` to `mc check` by default

`mc check` now includes Cargo version-group validation that was previously only run by `mc step:validate`. This means `mc check` catches inconsistent workspace version groups in Cargo manifests that the lint step alone would miss.
