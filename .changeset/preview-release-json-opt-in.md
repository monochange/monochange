---
monochange_core: minor
monochange: patch
---

# Add `--release-json` opt-in for writing release records during preview

`monochange preview` (and any dry-run `PrepareRelease`) no longer writes the release record (`.monochange/releases/<hash>/release.json`) by default. Pass `--release-json` to opt back into writing it during a dry run.

```bash
# preview without touching the release record
monochange preview

# preview that also writes release.json
monochange preview --release-json
```

Dry-run output now notes when the release record was skipped so the opt-in is discoverable. The git-ignored local manifest cache (`.monochange/local/release-manifest.json`) is still written in dry runs for downstream step rendering.
