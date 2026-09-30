---
monochange_core: patch
monochange_python: patch
---

# Include canonical Python names in dependent release plans

A producer named `PY_Core` now matches declared dependencies written as `py-core` or `py_core`, restoring dependent bumps and dependency ordering while preserving the producer's native name. A producer-only minor changeset can therefore release its consumers with their configured propagation policy instead of omitting them from the plan.

Adapters can provide a canonical dependency-name alias through `PackageRecord.metadata` using the shared `PACKAGE_DEPENDENCY_NAME_METADATA_KEY` constant. `materialize_dependency_edges` matches the alias for consumers in the producer's ecosystem and emits each target ID once. This prevents a normalized Python alias from creating an unrelated Cargo dependency edge with the same spelling. Exact native-name matching retains its existing behavior, including adapters without an alias.
