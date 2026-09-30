---
monochange: patch
monochange_config: patch
monochange_deno: minor
---

# Prepare releases from Deno JSONC manifests

Repositories using `deno.jsonc` can now initialize, validate, discover, and prepare releases with comments and trailing commas. Validation recognizes the discovered JSONC manifest instead of requiring a nonexistent `deno.json`; dependency synchronization retains the original comments and layout.

```sh
monochange init
monochange step validate
monochange prepare --dry-run
```

These commands now accept supported Deno JSONC manifests throughout the workflow. The Deno adapter also exposes `parse_manifest_contents(contents: &str) -> Result<serde_json::Value, serde_json::Error>` for callers that need the same comment and trailing-comma handling.

Malformed block comments and tokens split by comments are rejected instead of being silently accepted or joined into a different value.
