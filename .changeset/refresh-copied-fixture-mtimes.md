---
"monochange_test_helpers": patch
---

# Detect fixture edits that keep the same file size and modification time

`copy_directory` in `monochange_test_helpers` used `fs::copy`, which preserves the source modification time. Fixtures that stage a `before` tree, commit it, and then overwrite it with an `after` tree produced files whose contents differed but whose size and modification time were effectively unchanged.

Git trusts its stat cache for entries where size and modification time match, so `git add` sometimes recorded the previous contents. A fixture that bumps `version = "0.1.0"` to `version = "0.1.1"` has an identical byte length, which made the committed manifest depend on which nanosecond the copy landed in:

```
before: version = "0.1.0"   (43 bytes)
after:  version = "0.1.1"   (43 bytes)
```

`copy_directory` now sets a fresh modification time on every copied file, so `git add` always re-hashes the copied contents. Test helpers are internal, but this removes an intermittent failure for any fixture using the `before`/`after` layout.
