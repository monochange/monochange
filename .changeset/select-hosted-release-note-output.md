---
monochange_github: minor
monochange_hosting: minor
---

# select the changelog output used for hosted releases

Hosted release providers can now publish a configured changelog output instead of always using the implicit developer changelog. This lets a repository keep detailed package notes for maintainers while selecting concise user-facing notes for GitHub, GitLab, Gitea, or Forgejo releases.

```toml
[source.releases]
changelog_output = "user_notes"

[changelog.outputs.user_notes]
stream = "user"
path = "release-notes/{{ id }}/{{ version }}.md"
format = "monochange"
mode = "release"
targets = ["app"]
```

The default remains `changelog_output = "default"`, preserving existing release bodies. Configuration validation rejects unknown output names, and provider rendering only falls back to an empty body when the selected stream genuinely has no notes—it never substitutes developer content from another stream.
