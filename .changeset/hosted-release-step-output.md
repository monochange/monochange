---
monochange: feat
---

# Add machine-readable output and OIDC audience to hosted release steps

`monochange step commit-release` now accepts `--format text|json|json-min|md`, and its JSON `release_commit` object includes `verified` when the hosted backend created the commit, so CI can read the commit and its GitHub verification without parsing text:

```bash
monochange step commit-release --commit-backend hosted --format json
```

```json
{
	"release_commit": {
		"commit": "4f2c9e1",
		"verified": true,
		"status": "completed"
	}
}
```

`verified` is omitted for local commits and dry runs, where it is unknown.

`OpenReleaseRequest` accepts the `oidc_audience` input that `CommitRelease` already had, so both hosted steps can request tokens for a custom audience:

```bash
monochange step open-release-request --backend hosted --oidc-audience release.example.com
```

In a workflow, pass it through the step inputs: `{ type = "OpenReleaseRequest", backend = "hosted", inputs = { oidc_audience = "release.example.com" } }`.
