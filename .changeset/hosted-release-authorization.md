---
monochange_app: fix
---

# Protect hosted release branches and pull requests

Enforce hosted release authorization before creating or moving any branch. Every hosted commit must name a `<prefix>/release` branch distinct from both the requested base and the repository default. Older clients may still create or fast-forward a valid release branch without naming a base; replacing an existing release branch requires a named base whose head still matches the prepared commit.

Hosted pull request updates now require a server-signed marker bound to the PR number, node ID, bot creator, repository IDs, head, and base. Requests cannot adopt an unrelated PR or enable auto-merge on it. Existing release PRs without a marker must be closed and recreated once; rotating the deployment JWT signing secret also requires recreating open release PRs. If GitHub creates a release PR but its marker update fails, close the newly created PR identified in the error before retrying.
