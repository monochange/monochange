---
"monochange": minor
---

# Keep long-lived release pull requests openable

A release pull request that stayed open for days could grow until GitHub refused to create its replacement, which left the release blocked until someone worked out why. The cause was the pull request body: monochange inlined every release note for every release target and only GitHub's create call enforced a size limit, so the body grew silently through updates.

Release pull request bodies are now bounded. Two settings control the result, and the default fixes the failure without any configuration change:

```toml
[source.pull_requests]
# "full" inlines the release notes (the default). "summary" renders only the
# prepared-release header, the target list, and the changelog paths.
body_style = "full"
# Optional. Defaults to the provider's own limit, which is 65536 characters for GitHub.
# max_body_chars = 65536
```

When the notes do not fit, the body keeps the header and the target list, drops entries from the end, and ends with a pointer to the changelog files that still carry everything:

```text
## Full release notes

3 entries omitted to fit the body limit. The complete notes are in:

- `crates/app/CHANGELOG.md`
- `crates/core/CHANGELOG.md`
```

The step reports the same thing, so the shortening is visible before the next create call rather than at the point GitHub rejects it:

```text
release request warnings:
- release request body shortened to 500 characters (from 1498); 3 release-note entries were dropped.
  Set `[source.pull_requests].max_body_chars` or `[source.pull_requests].body_style = "summary"` to control the limit.
```

No release notes are lost: they are also written to `changelog.md`, the per-crate changelogs, the GitHub release body, and every configured changelog output. Set `body_style = "summary"` if you would rather keep the review surface small on purpose, and raise `max_body_chars` for a self-hosted provider that accepts a larger body.
