---
monochange: patch
monochange_config: patch
---

# enforce configured type-scoped changeset policies

Custom changelog types can now enforce their configured changeset body policy during `monochange check`. This makes release-note contracts such as separate user impact, developer notes, and rollout sections auditable before a release is planned.

For example, repositories can map an app-specific type into a custom release-note section and validate its content:

```toml
[changelog.sections]
app_features = { heading = "App features", priority = 10 }

[changelog.types]
app_feature = { bump = "minor", section = "app_features" }

[lints.rules]
"changesets/types/app_feature" = {
  level = "error",
  required_bump = "minor",
  required_sections = ["User impact", "Developer notes"],
}
```

Previously, Monochange accepted and validated the `changesets/types/app_feature` configuration but did not register a matching lint runner. A changeset missing `Developer notes` therefore passed `monochange check`. The changeset lint suite now receives the configured type names and creates a scoped runner for each one, so the same changeset fails with the configured dynamic rule id and a concrete missing-section error.

The static lint catalog and existing `changesets/bump/<severity>` policies are unchanged.
