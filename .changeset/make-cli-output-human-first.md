---
monochange: patch
---

# Make human-readable text the default CLI result

monochange commands now print concise text unless you explicitly request Markdown or JSON. `monochange step config` prints a short workspace summary instead of the complete resolved configuration.

```bash
# human summary
monochange step config

# complete structured configuration
monochange step config --format json
```

Explicit `--format markdown` output stays as raw Markdown even when stdout is a terminal. `--jq` now requires `--format json` or `--format json-min`, so it cannot run a mutating command and only then discover that the result was not JSON.

`--quiet` now controls output only. It no longer silently changes a real operation into a dry run. Add `--dry-run` explicitly when you need both behaviors:

```bash
monochange run release --dry-run --quiet
```

Configured workflows that intentionally set their own `format` default keep that explicit choice.
