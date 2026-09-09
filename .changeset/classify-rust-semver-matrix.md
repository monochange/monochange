---
"@monochange/cli": minor
"@monochange/skill": minor
monochange: minor
monochange_analysis: minor
monochange_cargo: breaking
monochange_config: minor
monochange_core: breaking
monochange_schema: minor
---

# classify Rust compatibility across configured feature and target matrices

Cargo packages can opt into cargo-semver-checks during semantic change classification:

```toml
[ecosystems.cargo.semver_checks]
enabled = true
timeout_seconds = 300

[[ecosystems.cargo.semver_checks.matrix]]
name = "default"
feature_mode = "default"

[[ecosystems.cargo.semver_checks.matrix]]
name = "all-features"
feature_mode = "all"
```

monochange materializes complete before and after repository trees, runs every configured feature/target cell with an isolated Cargo target directory, and reports the cargo-semver-checks version, exact cell inputs, status, outcome, minimum bump, lint ids, lint titles, and authoritative references. Any proven break proposes `major`, while failed or skipped cells preserve the conservative syntax fallback and require review. A complete non-breaking matrix can replace syntax-derived modifications and removals; added public syntax remains `minor` evidence because cargo-semver-checks does not enable every additive lint by default.

The change-classification JSON schema is now version `3`. Each finding's `coverage` may include a `checks` array with machine-readable analyzer sub-checks. Pull request comments and text reports render the same cell outcomes and diagnostic summaries.

`monochange_core::EcosystemSettings` now includes `semver_checks`, and `monochange_cargo::CargoSemanticAnalyzer` is no longer a unit struct. Construct it with `monochange_cargo::semantic_analyzer()` for the disabled default or `monochange_cargo::semantic_analyzer_with_settings(settings)` for an explicit matrix.

cargo-semver-checks executes Cargo builds at both endpoints, including build scripts and procedural macros. Run semantic classification only for code you are prepared to execute and without elevated CI or publishing credentials.
