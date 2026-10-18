---
"monochange": patch
---

# Show hosted release commit verification in text and Markdown reports

The `CommitRelease` step already records GitHub's verification result in the `verified` field of its JSON report. The text and Markdown summaries now show it too, so a run that used the hosted backend says whether GitHub verified the release commit without reading the JSON.

Local commits and dry runs leave `verified` unset and render as before.

**Before (text output):**

```text
Release commit
  1234567  chore(release): publish
  2 tracked paths · already-exists
```

**After (text output):**

```text
Release commit
  1234567  chore(release): publish
  2 tracked paths · already-exists · verified
```

The Markdown summary adds a `- **Verified:** yes` or `- **Verified:** no` line. When GitHub does not verify a hosted commit, the hosted backend now fails the step and leaves the release branch where it was.
