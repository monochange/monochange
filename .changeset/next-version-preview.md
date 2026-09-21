---
"monochange": minor
---

# Check the next version with `monochange next`

Answering "what version will this release be" required knowing that `monochange step display-versions` existed. There is now a top-level command for it:

```bash
monochange next
```

```text
group versions:
- sdk: 1.1.0
package versions:
- cargo:crates/sdk-a/Cargo.toml: 1.1.0
- cargo:crates/sdk-b/Cargo.toml: 1.1.0
- cargo:crates/tool/Cargo.toml: 1.0.1
```

It reports one version per release group plus one version for every package that releases independently, and accepts `--format text|json|json-min|md` for scripting:

```bash
monochange next --format json
```

```json
{
	"packages": {
		"cargo:crates/sdk-a/Cargo.toml": "1.1.0",
		"cargo:crates/tool/Cargo.toml": "1.0.1"
	},
	"groups": {
		"sdk": "1.1.0"
	}
}
```

`monochange next-versions` is an alias that resolves to the same command. Both are read-only aliases for `monochange step display-versions`: no `release.json`, no prepared-release cache under `.monochange/local/`, and no changes to manifests, changelogs, or changesets, so the command leaves a clean working tree and is safe in a pre-commit check or a reporting-only CI job.

For contrast, `monochange versions list` reports the versions recorded in the workspace today, while `monochange next` reports the versions planned from pending changesets.
