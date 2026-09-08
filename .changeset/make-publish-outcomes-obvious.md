---
monochange: patch
monochange_core: patch
monochange_publish: breaking
---

# Make package publishing outcomes obvious

Package publishing now starts human-readable output with a distinct outcome such as `Published 3 packages`, `Would publish 3 packages`, `No packages need publishing`, or `Publishing failed for 1 package`. Published or planned package versions appear immediately below the headline, with explicit counts for versions that already exist, are blocked, failed, or were not attempted.

Pass `--show-all` to `PublishPackages` or `PlaceholderPublish` when you need every package's status, trusted-publishing metadata, command, stdout, and stderr. JSON and template output always retain all package rows.

`PackagePublishSummary` now reports each domain status directly. Replace `expected`, `succeeded`, and `skipped` with `total()`, `published`, `already_exists`, `blocked`, and `not_attempted` as appropriate:

```rust
let summary = report.summary();
assert_eq!(summary.total(), report.packages.len());
assert_eq!(summary.published, 3);
assert_eq!(summary.already_exists, 2);
```
