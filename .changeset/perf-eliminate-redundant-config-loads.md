---
"monochange": patch
"monochange_config": patch
---

# Eliminate redundant config loads and deduplicate glob validation in `mc check`

The `mc check` command was loading workspace configuration three times: once in `run_check_command`, once in `validate_workspace`, and once in `validate_versioned_files_content`. Each load discovers and parses all manifest files, so the triple-load was wasteful.

This change adds `validate_workspace_with_config` and `validate_versioned_files_content_with_config` variants that accept a pre-loaded `&WorkspaceConfiguration`, avoiding redundant I/O.

Additionally, versioned file glob patterns (e.g. `**/*.pubspec.yaml`) are now deduplicated across all packages. In repos with 50+ packages, each inheriting the same ecosystem-level glob pattern, the glob was expanded separately for every package — walking the entire repo directory tree each time. Deduplicating to validate each unique glob once eliminates this O(P×G) blowup, reducing `mc check` time from ~28s to ~3s on large monorepos (a ~90% improvement).

Also adds a progress message ("Validating workspace…") during the validation phase of `mc check`.
