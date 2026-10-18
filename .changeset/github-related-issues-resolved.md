---
monochange_github: fix
---

# Link only real issues as a changeset's related issues

GitHub changeset context recorded every `#N` in a pull request body as a related issue. Pull requests mentioned in prose, numbers from other repositories written as `repo#N`, and numbers inside code were published under "Related issues", and `CommentReleasedIssues` then posted "Released in …" on them.

`enrich_changeset_context` now asks GitHub what each referenced number is, with one `issueOrPullRequest` lookup per batch of 100 numbers, and keeps only `Issue` nodes. Each kept `HostedIssueRef` gets the issue's `title` and URL. Related issues come from the review request that introduced the changeset, so a pull request that only reformatted or reverted `.changeset/*.md` files no longer adds its references to every changeset it touched.

Reference extraction also changed:

- `repo#N` without an owner is treated as a reference to another repository and skipped, the same as `owner/other-repo#N`.
- References inside fenced code blocks and inline code spans are ignored.
- `#12abc` is no longer read as `#12`.

If the lookup fails, only references made with a closing keyword (`Closes #7`) are kept, without titles. Consumers that worked around the bug by writing full pull request URLs in descriptions can keep doing so; nothing needs to change.
