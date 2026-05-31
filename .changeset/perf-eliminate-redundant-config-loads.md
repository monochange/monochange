---
"monochange": patch
---

Eliminate redundant `load_workspace_configuration` calls in `mc check`

The `mc check` command was loading the workspace configuration three times:
once in `run_check_command`, once in `validate_workspace`, and once in
`validate_versioned_files_content`. Each load discovers and parses all
manifest files (e.g., 54+ pubspec.yaml files in large monorepos), so the
triple-load was wasteful.

This change adds `validate_workspace_with_config` and
`validate_versioned_files_content_with_config` variants in
`monochange_config` that accept a pre-loaded `&WorkspaceConfiguration`,
avoiding redundant I/O. The `mc check` command now loads configuration
once and passes it through to both validation calls.

Also adds a progress message ("Validating workspace…") during the
validation phase of `mc check`, using the same output format detection
as the lint progress reporter.