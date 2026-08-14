---
monochange_publish: patch
---

# Report every expected package after publishing stops on failure

Sequential package publishing remains fail-fast, but its report now includes every package that was expected. Packages not attempted after the first failure are recorded as blocked with an explanation, reported as skipped in progress totals, and remain eligible for a resumed publish.

**Before:**

```text
◆ Publish complete: 13 packages, ✅ 12 published, ⏭️ 0 skipped, ❌ 1 failed
```

**After:**

```text
◆ Publish complete: 45 expected, ✅ 12 succeeded, ❌ 1 failed, ⏭️ 32 skipped
```

The library also exposes a derived summary without changing the persisted report schema:

```rust
// before
let report: PackagePublishReport = execute_publish_requests(...).await?;

// after
let report: PackagePublishReport = execute_publish_requests(...).await?;
let summary: PackagePublishSummary = report.summary();
assert_eq!(summary.expected, 45);
assert_eq!(summary.succeeded, 12);
assert_eq!(summary.failed, 1);
assert_eq!(summary.skipped, 32);
```

Publish errors now include these aggregate counts and retain the failed package's command diagnostics. Command error rendering preserves stdout-only failures and clearly labels both streams when both are available.
