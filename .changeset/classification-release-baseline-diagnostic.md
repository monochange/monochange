---
monochange_classification: patch
---

# Correct missing release-baseline diagnostics

Report `no matching release tag found for this package` when classification has no release baseline. The diagnostic no longer claims that the tag lookup filters by Git reachability.
