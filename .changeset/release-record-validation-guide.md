---
monochange_app: fix
---

# Clarify release-record validation in the CLI guide

The `CommitRelease` reference now keeps mismatch protection enabled in its formatting example. It explains which prepared fields are checked, how rejected records preserve neighboring release records, and when deliberate recovery with `update_release_json = true` is appropriate. Whitespace and JSON key ordering require no configuration change.
