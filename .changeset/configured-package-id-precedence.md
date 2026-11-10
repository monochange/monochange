---
"monochange_config": patch
"monochange": patch
---

# Prefer configured package IDs over shared published names

`monochange preview` and `resolve_package_reference` now honor an exact configured package ID before matching published names. Repositories can keep a Rust ID unprefixed while assigning the same-name Dart package a distinct ID:

```toml
[package.shared]
path = "crates/shared"

[package."dart:shared"]
path = "packages/shared"
type = "dart"
```

A changeset targeting `shared` selects Rust and one targeting `dart:shared` selects Dart. This previously failed as ambiguous despite the explicit IDs. Unconfigured published names still resolve when unique and still report ambiguity when they identify multiple packages. Canonical IDs and manifest/directory paths continue to resolve in both direct and cached lookup paths.
