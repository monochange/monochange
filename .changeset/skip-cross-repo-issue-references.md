---
monochange_core: patch
monochange_github: patch
monochange: patch
---

# skip cross-repository issue references and tolerate missing issues in release comments

`monochange step comment-released-issues` used to fail the whole release automation when a released changeset referenced an issue in another repository (GitHub `owner/repo#123` shorthand). The issue-reference extractor recognized the `owner/repo` prefix but discarded it, so a link such as `[actions/toolkit#2048](https://github.com/actions/toolkit/issues/2048)` was resolved as a monochange issue and the GitHub API returned 404 — which failed the `release-post-merge` workflow after the release had already published.

Issue references are now scoped to the configured repository: only bare `#123` references and `owner/repo#123` references whose prefix matches the configured repository are attributed to the release; references for other repositories are ignored.

```bash
# before — failed the release post-merge workflow with
# config error: GitHub API GET /repos/monochange/monochange/issues/2048/comments failed: status 404
monochange step comment-released-issues --from-ref HEAD --auto-close-issues

# after — cross-repository references are skipped and missing issues no longer fail the step
monochange step comment-released-issues --from-ref HEAD --auto-close-issues
```

When a referenced issue cannot be resolved (deleted or made private after the release record was written), the step now reports it as `skipped_missing` in the step outcome instead of failing, so one stale reference can no longer block a release from being announced.
