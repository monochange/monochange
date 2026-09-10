---
"monochange": fix
---

# Detect release records at merge commits from the first parent only

Release-record discovery diffed each commit against every parent with `git diff-tree -m`. A merge of a branch that was cut before a release therefore re-reported that release's `.monochange/releases/<hash>/release.json` relative to its older second parent, and the merge commit itself resolved as the release-record commit at distance 0. `release-record --from HEAD` then reported a fresh release for every post-release merge of a pre-release branch, and `tag-release` failed with `tag ... already points to commit ...` even though the existing tags were correct and nothing needed to move — which failed the `release-post-merge` CI job. Any pull request opened before a release PR that merges right after it triggers this, which the merge queue makes routine.

Discovery now diffs each commit against its first parent only, the parent the commit landed through. A merge of a pre-release branch no longer looks like a new release commit, while a release branch that adds the record itself is still detected when it merges, and a commit that deletes the record stays excluded.
