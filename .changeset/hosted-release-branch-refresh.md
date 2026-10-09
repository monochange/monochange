---
monochange_app: fix
---

# Refresh existing release branches from hosted release commits

`POST /api/release-commits` returned `409 Conflict` whenever the release branch existed at any commit other than the run's base commit. After the first run the branch always holds the previous release commit, so every later push failed until the release pull request was merged and its branch deleted.

The release branch is rebuilt from the base branch on every run, so the endpoint now replaces it (a forced ref update) when the request's `base_branch` still points at `base_commit`. The race the old guard covered is kept: when the base branch has moved, the run is stale and the endpoint returns `409` with a `base branch ... moved` message, leaving the release to the run for the newer commit. Requests without `base_branch` (CLIs before this release) and requests that target the base branch itself can still only create the branch or fast-forward it from `base_commit`. The branch check now runs before any blob, tree, or commit is written.

The endpoint also reported `verified: true` for every commit; it now returns GitHub's verification result.
