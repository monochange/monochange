---
monochange: minor
monochange_core: minor
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
monochange_forgejo: patch
monochange_schema: patch
---

# Override the release commit subject

`[source.pull_requests]` accepts a new optional `commit_subject` key that sets the release commit subject independently of the release pull request title. When omitted, the commit subject keeps falling back to `title`, so existing configs are unchanged.

Repos that prefix release commits with an emoji no longer have to put the emoji in the pull request title:

```toml
[source.pull_requests]
title = "chore(release): prepare release"
commit_subject = "🔖 chore(release): prepare release"
```

The override applies to the local `CommitRelease` step and to the commit that `OpenReleaseRequest` places on the release branch for GitHub, GitLab, Gitea, and Forgejo. The configuration schema gains the optional `commit_subject` property.
