---
monochange: patch
---

# speed up versioned-file glob expansion

Versioned-file and workspace-member glob expansion now goes through one shared workspace-aware glob helper. Broad globs such as `**/pubspec.yaml` skip ignored local tooling, dependency, and vendor trees including `.fvm`, `.repos`, `node_modules`, and `target`, while explicit literal paths can still target ignored directories when that is intentional.

This prevents release planning from scanning and rewriting unrelated checked-out toolchains or nested repositories, and adds regression coverage plus a benchmark for the ignored-tree case.

## batch glob expansion

Release planning now prewarms a versioned-file path cache and expands all configured globs in a single workspace walk using `workspace_glob_files_many`. Configs with many broad patterns (e.g. 20 `**/metadata-*.json` rules) no longer pay one walk per pattern — the batch API collapses N walks into one shared traversal, matching each candidate path against all compiled patterns in a single pass.
