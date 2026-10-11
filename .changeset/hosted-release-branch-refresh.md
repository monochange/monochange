---
monochange_app: fix
---

# Refresh existing release branches from hosted release commits

`POST /api/release-commits` returned `409 Conflict` whenever the release branch existed at any commit other than the run's base commit. After the first run the branch always holds the previous release commit, so every later push failed until the release pull request was merged and its branch deleted.

The release branch is rebuilt from the base branch on every run, so the endpoint now replaces it with a forced ref update. Creating the branch or fast-forwarding it from `base_commit` is always allowed. A forced replacement is allowed only when all of these hold:

- the request names `base_branch` (CLIs before this release do not);
- the branch has the release-branch shape `<branch_prefix>/release`;
- the branch is neither `base_branch` nor the repository's default branch (read from `GET /repos/{owner}/{repo}` only when a force is needed);
- `base_branch` still points at `base_commit`.

When the base branch has moved, the run is stale and the endpoint returns `409` with a `base branch ... moved` message, leaving the release to the run for the newer commit. Any other refused replacement returns `409` with a `release branch ... moved` message, so a caller cannot force `main` or another non-release branch onto a different commit. The branch check runs before any blob, tree, or commit is written.

Both hosted endpoints now reject branch names that are not plausible Git refs with `400`: empty names, `..`, `//`, `@{`, a leading `/` or `-`, a trailing `/`, `.`, or `.lock`, control characters, and characters that Git forbids or that would change the API URL (`space ~ ^ : ? * [ \ # %`).

The endpoint also reported `verified: true` for every commit; it now returns GitHub's verification result.
