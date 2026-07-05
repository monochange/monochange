---
monochange: patch
monochange_config: patch
---

# exit successfully after fixing all check issues

`monochange check --fix` now re-checks manifests after applying fixes and exits successfully when no errors remain. Changeset heading length diagnostics now refer to the changeset header when the first body line is a heading.
