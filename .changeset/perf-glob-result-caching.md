---
monochange_dart: patch
monochange_cargo: patch
monochange_deno: patch
monochange_npm: patch
monochange_core: patch
---

# Eliminate redundant directory traversals and optimize path operations

Each ecosystem adapter (Dart, Cargo, Deno, npm) previously called `find_all_manifests` twice during package discovery — once to find workspace manifests and again to find all manifests for standalone packages. This doubled the wall-clock time for large monorepos.

The fix refactors each adapter to call `find_all_manifests` once and reuse the results for both workspace and standalone discovery, and also removes the now-unused `find_workspace_manifests` helper functions.

Additionally, `normalize_path` now skips `fs::canonicalize` when the path has no `.` or `..` components, and `ignored_discovery_dir_name` checks only the file name instead of all path components.

Benchmark results (51-package Dart monorepo):

- Before: ~40ms
- After: ~8.4ms (79% improvement)
