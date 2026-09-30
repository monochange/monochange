---
"monochange_github": fix
---

# Keep release pull request commits verified when the release deletes files

Repositories that set `[source.pull_requests].verified_commits = true` lost the verified release commit whenever the release removed files, which is every release that consumes `.changeset/*.md` files. GitHub rejected the tree request with `status 422; Must supply either tree.sha or tree.content`, so the provider logged `falling back to regular release pull request commit` and left the unverified git commit on the release branch.

Deleted paths are now sent to GitHub's create-tree API with an explicit `"sha": null`, which is how GitHub removes a path from the new tree, so the verified replacement commit is created and the release branch moves to it:

```json
{
	"path": ".changeset/feature.md",
	"mode": "100644",
	"type": "blob",
	"sha": null
}
```

No configuration change is needed; the next release pull request opened from GitHub Actions gets the verified commit.
