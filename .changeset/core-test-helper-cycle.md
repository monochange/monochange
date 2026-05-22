---
monochange_core: patch
---

# Remove core test helper dependency

Remove the monochange_core test dependency on monochange_test_helpers so package publish ordering no longer sees a development dependency cycle between the two crates.
