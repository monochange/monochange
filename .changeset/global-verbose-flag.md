---
"monochange": feat
---

# Add a global `--verbose` flag

`--verbose` (`-v`) now works on every command. It shows complete lists instead of the first 20 entries, full command-step logs and changeset details, the timing of every phase of every step, and progress notes such as the verified release commit that was created or each package that was published. It replaces the `check`-only `--verbose` flag, so `monochange check --verbose` keeps working.

```bash
monochange --verbose run release --dry-run
```

`--verbose` does not enable maintainer tracing; `--log-level` still does.
