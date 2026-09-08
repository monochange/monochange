---
"@monochange/cli": patch
monochange: patch
monochange_core: patch
monochange_lint: patch
monochange_cargo: patch
monochange_dart: patch
monochange_npm: patch
---

# `monochange check --fix` no longer replaces whole manifests with a single line

Running `monochange check --fix` no longer replaces whole manifests with a single line

Running `monochange check --fix` with manifest lint rules enabled could replace an entire `Cargo.toml` with a one-line fragment such as `repository = "..."`, deleting every other field, table, and comment in the file. The `cargo/manifest-repository` rule (with or without `allow_workspace_inheritance`) triggered this whenever it rewrote a repository value, and the `cargo/dependency-field-order`, `cargo/internal-dependency-workspace`, and `cargo/sorted-dependencies` fixes carried the same hazard.

The cargo lint fixes now rewrite the whole manifest from a mutated copy of the parsed document, so unrelated content always survives and toml_edit keeps the surrounding formatting. Whole-file rewrites are additionally validated against the target ecosystem's own manifest parser (`LintSuite::validate_contents`) before anything is written; a rewrite that would produce an unparseable manifest is skipped and the original file is kept. Manifest-level fixes across the npm and dart suites now use the same explicit `LintFix::document` constructor, and new regression tests cover every cargo lint rule's fix output plus an end-to-end `check --fix` convergence run.
