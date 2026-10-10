---
"monochange": "minor"
---

# Report hosted release commit verification in the release-commit result

`CommitRelease` now surfaces whether GitHub marked the hosted release commit as verified in the command's JSON and human-readable reports. The hosted backend previously discarded the app's verification decision, so a repository that required signed commits would only discover an unverified commit when the release branch protection rejected the merge.

The `verified` field appears in `CommitReleaseReport` (the JSON output of the `CommitRelease` step) and in the text and Markdown summaries as **Verified: yes** / **not verified** / **no verification requested**. A hosted commit with `verified: false` will still be created, but the report no longer pretends it was verified by GitHub.

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

```json
{
	"releaseCommit": {
		"subject": "chore(release): publish",
		"body": "body",
		"commit": "1234567890abcdef",
		"trackedPaths": ["Cargo.toml", "CHANGELOG.md"],
		"dryRun": false,
		"status": "already_exists",
		"verified": true
	}
}
```
