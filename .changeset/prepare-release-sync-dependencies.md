---
monochange: patch
---

# Sync dependency constraints during release preparation

`PrepareRelease` now automatically refreshes internal workspace dependency constraints after it bumps package versions. For example, a Dart package depending on a sibling that was released from `1.0.0` to `1.1.0` is updated from `sibling: ^1.0.0` to `sibling: ^1.1.0` during release preparation instead of requiring a separate `monochange versions sync` command.
