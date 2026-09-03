---
monochange: patch
---

# ignore deleted release records when discovering the release commit

Release-record discovery listed release-record paths through `git diff-tree` without excluding deletions, so a commit that _removed_ a release record — for example when reverting a release preparation — made `monochange step publish-packages`, `tag-release`, and `release-record` fail with:

```
discovery error: failed to read `.monochange/releases/<hash>/release.json` at commit `<sha>`: fatal: path … does not exist in '<sha>'
```

Discovery now excludes deleted paths (`--diff-filter=d`), so reverting a release preparation walks past the deleting commit to the release commit that actually added the record.

```bash
# before — failed on any commit that deleted a release record
monochange step publish-packages --dry-run
# discovery error: failed to read .monochange/releases/…/release.json at commit …

# after — deleted records are skipped and discovery walks to the record commit
monochange step publish-packages --dry-run
```
