---
monochange: patch
monochange_dart: patch
monochange_cargo: patch
monochange_deno: patch
monochange_npm: patch
---

# Eliminate redundant directory traversals in ecosystem discovery

Each ecosystem adapter (Dart, Cargo, Deno, npm) previously called `find_all_manifests` twice during package discovery — once to find workspace manifests and again to find all manifests for standalone packages. This doubled the wall-clock time for large monorepos.

The fix refactors each adapter to call `find_all_manifests` once and reuse the results for both workspace and standalone discovery, and removes the now-unused `find_workspace_manifests` helper functions.

Benchmark results (51-package Dart monorepo):

- Before: ~40ms
- After: ~14ms (65% improvement)
