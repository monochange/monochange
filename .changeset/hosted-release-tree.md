---
monochange: feat
monochange_github: feat
---

# Build hosted release commits from a tree the workflow uploads

Hosted `CommitRelease` no longer sends the release files to the monochange app. The CLI uploads them to the repository as git blobs and a tree with the workflow's `GITHUB_TOKEN` (or `GH_TOKEN`), then sends only the tree id, the base commit, and the commit message. The app commits that tree through the GitHub App installation, so the commit is still verified and still triggers workflows, and the request carries the commit message instead of every changelog in full. Binary files and executable or symlink modes now survive hosted commits, because blobs are uploaded as base64 with the file mode `git add` would use.

Give the release job `contents: write` so `GITHUB_TOKEN` can create blobs and trees. Uploading them pushes no branch and starts no workflow:

```yaml
permissions:
  contents: write
  pull-requests: read
  id-token: write
```

Without a token, or when the token gets `403` from the blob API, the CLI prints a warning and falls back to sending the file contents, as before.

`monochange_github` adds `read_release_tree_files` and `create_release_tree`, which return `ReleaseTree::Created(tree)` or `ReleaseTree::Unavailable(reason)` so callers can fall back instead of failing.
