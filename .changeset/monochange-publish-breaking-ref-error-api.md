---
"monochange_publish": major
---

# Replace the Dart protected-publishing warning helper with the pub.dev ref preflight

> **Breaking:** `dart_protected_publishing_warning` was removed from `monochange_publish`. Call `pub_dev_trusted_publishing_ref_error` instead. Both take `(&PublishRequest, &BTreeMap<String, String>)` and return `Option<String>`, but the new preflight detects every non-tag GitHub Actions run ref (not just `workflow_dispatch` events) and returns the message monochange fails the publish run with, so callers should surface it as an error rather than a warning.

```rust
// Before
let warning = dart_protected_publishing_warning(&request, &env_map);

// After
let error = pub_dev_trusted_publishing_ref_error(&request, &env_map);
```
