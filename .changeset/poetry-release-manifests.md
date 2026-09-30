---
monochange_python: patch
---

# Keep Poetry manifests and lock commands consistent with release plans

Poetry-only packages now write the planned version to `[tool.poetry].version`. Internal dependency constraints in Poetry runtime and dependency-group tables update during preparation while preserving extras, markers, comments, and path or Git source metadata. Packages using `[project]` keep PEP 621 precedence, including dynamic versions.

Producer and dependency names now use Python's canonical matching rules. A producer named `PY_Core` matches constraints written as `py-core` or `py_core`, so both Poetry and PEP 621 manifest updates include that package.

For example, a release of an internal dependency to `1.1.0` updates its existing constraint without removing optional metadata:

```toml
[tool.poetry.dependencies]
internal = { version = ">=1.1.0", extras = ["http"] }
```

Inferred Poetry lock commands now run `poetry lock`, supported by Poetry 2, instead of the removed `--no-update` option. Poetry 1 installations that need the old option can configure `lockfile_commands` explicitly.
