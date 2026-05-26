---
monochange: minor
monochange_core: minor
monochange_dart: minor
monochange_npm: minor
"@monochange/skill": patch
---

# Add `mc versions` for internal dependency synchronization

Add `mc versions`, a top-level CLI command that synchronizes internal workspace dependency constraints with each package's canonical version. This is intended for migrating existing monorepos to monochange and for keeping Dart/npm internal dependency references consistent after version changes.

Run `mc versions --dry-run` to preview file edits, or `mc versions` to apply them. The command prints each dependency update and the manifest file that would be changed. Use `mc versions --dry-run --format json` for CI and scripts that need structured output.

- **`VersionStrategy` enum** in `monochange_core` controls constraint format: `Default`, `Exact`, `Caret`, `Compatible`.
- **`DependencySyncChange` struct** in `monochange_core` reports dependency name, section, old value, and new value.
- **`sync_internal_dependency_versions()`** in `monochange_dart` scans pubspec `dependencies`, `dev_dependencies`, and `dependency_overrides` for internal workspace references. Under `resolution: workspace`, eligible `path:` references are converted to versioned constraints.
- **`sync_internal_dependency_versions()`** in `monochange_npm` scans package.json for internal workspace dependencies, skipping `workspace:*` protocol references.
- **CLI command** `mc versions [--dry-run] [--strategy <default|exact|caret|compatible>] [--format <text|json>]` orchestrates discovery, version planning, unsupported ecosystem reporting, and per-ecosystem file updates.
- **Strategy precedence** defaults to package config → ecosystem config → ecosystem default. Passing `--strategy` overrides that fallback for the whole command.

Currently supports **Dart** and **npm** manifests. Other ecosystems are reported as skipped so migration runs show what still needs manual review.
