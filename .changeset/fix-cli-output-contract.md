---
monochange: patch
---

# Keep check failures and progress output safe for automation

`monochange check` now returns the same non-zero exit status for text, Markdown, JSON, and compact JSON when lint errors exist. JSON callers still receive the complete lint report on stdout.

```bash
monochange check --format json
echo $?
```

Before this change, a lint failure returned status `0` in JSON modes. It now returns status `1`, which lets CI stop on the failure without parsing `error_count` first.

Captured workflow progress no longer contains cursor-clearing escape sequences. `MONOCHANGE_NO_PROGRESS=1` also suppresses lint progress as documented, including explicit progress formats.
