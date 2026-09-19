---
"monochange": patch
---

# Keep floating tags on the stable release during a prerelease series

`tag-release` force-moved every configured `floating_tags` alias for every release target, so publishing `1.1.0-alpha.0` repointed `v1` and `latest` at a prerelease commit. Consumers resolving those aliases received a prerelease build without asking for one.

Floating aliases now stay pinned to the newest stable release while a SemVer prerelease is being tagged, and only a stable release moves them. The `floating_results` array for a prerelease target is empty, so a workflow inspecting the `tag-release` JSON report can see that no alias moved:

```json
{
	"tag_name": "core/v1.1.0-alpha.0",
	"operation": "created",
	"floating_results": null
}
```

Stable releases are unaffected: `core/v1.1.0` still repoints `v1` and `v1.1` to its own commit.
