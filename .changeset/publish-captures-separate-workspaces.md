---
monochange: fix
---

# publish all configured packages regardless of workspace layout

`monochange step publish-packages`, `monochange step placeholder-publish`, `monochange step publish-readiness`, and `monochange step plan-publish-rate-limits` no longer depend on the generic workspace walk when they resolve the packages selected by a release record. They now use the same configured-package discovery that release planning uses, so a package whose `monochange.toml` entry enables publishing is always captured — even when it lives in a separate Cargo/npm/Dart workspace that the repository-wide walk cannot see (for example a separate workspace under a gitignored directory, a nested git worktree, or an ignored directory name such as `target/` or `book/`).

Previously, a release record could list two packages while the publish run silently published only the one inside the primary workspace tree. The other package disappeared from the publish report, readiness output, and rate-limit batches without any warning. Publishing now follows the configuration: if the configuration says a package will be published, it is published.

Command:

```bash
monochange step publish-packages --dry-run --format json
```

**Before (output):** `beta` is configured with `publish = { enabled = true }` and listed in the release record, but lives in a separate workspace outside the discovery tree.

```json
{
	"package_publish": {
		"packages": [{ "package": "alpha", "status": "planned" }],
		"summary": { "expected": 1 }
	}
}
```

**After (output):**

```json
{
	"package_publish": {
		"packages": [
			{ "package": "alpha", "status": "planned" },
			{ "package": "beta", "status": "planned" }
		],
		"summary": { "expected": 2 }
	}
}
```

Configured packages that can no longer be discovered on disk now fail the run with a clear discovery error instead of being dropped silently.
