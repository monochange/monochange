---
"monochange": patch
"monochange_core": major
"monochange_github": minor
---

# Actually close the issues a release claims to close

`step comment-released-issues --auto-close-issues` never closed anything on a real release run, so issues named in release pull request bodies stayed open after publish even though each one received a "Released in" comment.

Three defects combined to hide that:

- The fresh-comment path in `comment_released_issues_with_client` reported the `closed` outcome without ever sending the `PATCH /issues/{n}` request. The close request only existed on the idempotent re-run branch, which no first release ever reaches.
- `build_issue_comment_results_for_source` accepted the caller's plans but re-planned internally through `HostedSourceAdapter::comment_released_issues`, silently discarding the `--auto-close-issues` decision. The flag only ever influenced dry-run output.
- The plan polarity was inverted for the GitHub closing-keyword behavior: issues referenced through closing keywords were trusted to have been closed by the forge at merge time, while plain mentions were marked for closure. GitHub only links the first issue of a comma-separated `Closes #1, #2` list, so the remaining keyword issues were closed by nobody, and non-actionable mentions would have been force-closed.

Closure now targets exactly the issues the release pull requests claim via closing keywords — including every entry of a comma-separated list — the close request is sent in the same run that posts the comment, and `--auto-close-issues` is honored in real runs instead of only dry runs. Plain mentions are never closed; add a closing keyword to a release pull request body when a mention should close with the release.

`HostedSourceAdapter` gains `comment_released_issues_with_plans` so provider adapters can post comments for caller-supplied plans; the existing default `comment_released_issues` delegates to it.

```toml
[[cli.release-comments.steps]]
type = "CommentReleasedIssues"
inputs = { format = "json", "from-ref" = "HEAD", "auto-close-issues" = true }
```
