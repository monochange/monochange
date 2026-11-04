---
monochange_app: fix
---

# Create hosted release trees with inline file contents

When a workflow sends release file contents instead of a prebuilt tree, `POST /api/release-commits` now creates the tree in one GitHub request with the contents inline. Previously the app made one installation-token `POST /git/blobs` request per file. Releases touching more than about 80 files hit GitHub's secondary rate limit for content creation, and the request outlasted the CLI's timeout. That broke the fallback the CLI uses when its own token cannot build the tree.

Deletions are still sent as `"sha": null`. A release with more than 4 MiB of file contents is split into tree requests chained through `base_tree`. A request that lists no files still creates a tree on the base commit, as before.
