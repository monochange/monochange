---
"monochange": fix
"monochange_python": fix
---

# Release explicitly configured Python packages

`monochange init` on a Python (uv or Poetry) repository generates `monochange.toml` entries that declare each workspace member with `type = "python"`. That configuration passed `monochange step validate`, but every release command failed:

```text
error[workspace.discovery_failed]: configured package `acme-report` at packages/report could not be discovered
```

The release-time workspace loader asks each ecosystem adapter to load its explicitly configured packages, and the Python adapter answered `Ok(None)` unconditionally — so a configured Python package was never loaded and `preview`, `prepare`, and `next` all refused to run. Validation never calls that loader, which is why the broken config looked healthy until the first real release.

The adapter now resolves the configured path to its `pyproject.toml` (accepting either the package directory or a direct manifest path), returns `Ok(None)` only when no manifest exists there, and otherwise loads the package through the same parser the discovery path uses. Both PEP 621 `[project]` and Poetry `[tool.poetry]` manifests load, and the failure is now limited to a genuinely missing or nameless manifest rather than every Python repository.

A Python package's own `version` field is still only rewritten when a `versioned_files` entry names it in `fields` — that is unchanged behavior, and the agent skill now documents it.
