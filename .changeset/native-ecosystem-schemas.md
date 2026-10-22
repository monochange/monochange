---
monochange_schema: feat
monochange_classification: feat
---

# Validate native package ecosystems and publish registries

Configuration, release-record, and classification schemas now accept `swift`, `cocoapods`, and `gradle` ecosystem values. Configuration and release records also recognize the `cocoapods_trunk` and `maven_central` registries.

Consumers validating native package declarations against generated schemas can use the new values directly:

```toml
[package.android]
path = "platforms/android"
type = "gradle"
```

Refresh cached schema assets before validating configurations or release records containing these values.
