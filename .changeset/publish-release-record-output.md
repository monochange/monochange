---
"monochange": fix
---

# Report provider releases and issue comments that come from a release record

`monochange step publish-release --from-ref <ref>` and `monochange step comment-released-issues` read a committed release record instead of preparing a release. Their results were dropped: text output was only ``command `step publish-release` completed``, and `--format json` printed that same sentence instead of JSON, so `monochange step publish-release --from-ref HEAD --format json | jq` failed.

```bash
monochange step publish-release --from-ref HEAD --draft --format json
```

The command now prints the planned or created provider releases and issue comments:

```json
{
	"releases": [
		{
			"provider": "github",
			"repository": "acme/app",
			"target_id": "main",
			"tag_name": "v1.2.0",
			"name": "v1.2.0 (2026-09-30)",
			"draft": true
		}
	],
	"issue_comments": []
}
```

Each release entry carries the complete `SourceReleaseRequest` fields; the example omits some for brevity.

Text output lists them under `Provider releases` and `Issue comments`.
