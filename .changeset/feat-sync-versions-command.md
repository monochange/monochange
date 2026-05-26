---
monochange: minor
monochange_core: minor
monochange_dart: minor
monochange_npm: minor
"@monochange/skill": patch
---

# Add `mc sync versions` command for internal dependency synchronization

Add a new CLI subcommand `mc sync versions` that synchronizes internal dependency version references across workspace packages to match each package's canonical version.

- **`VersionStrategy` enum** in `monochange_core` controls constraint format: `Default`, `Exact`, `Caret`, `Compatible`.
- **`DependencySyncChange` struct** in `monochange_core` reports what changed (dependency name, section, old value, new value).
- **`sync_internal_dependency_versions()`** in `monochange_dart` scans pubspec.yaml `dependencies`, `dev_dependencies`, and `dependency_overrides` for internal workspace references and computes the target version constraint. Under `resolution: workspace`, `path:` references are converted to versioned constraints.
- **`sync_internal_dependency_versions()`** in `monochange_npm` scans package.json for internal workspace dependencies, skipping `workspace:*` protocol references.
- **CLI subcommand** `mc sync versions [--dry-run] [--strategy <default|exact|caret|compatible>]` orchestrates discovery, version map building, and per-ecosystem sync.
- **`--dry-run`** flag shows what would change without writing files.

Currently supports **Dart** and **npm** ecosystems. Other ecosystems will be added in follow-up changes.
