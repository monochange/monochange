---
monochange_cargo: fix
monochange_config: fix
monochange_core: fix
monochange_dart: fix
monochange_deno: fix
monochange_forgejo: fix
monochange_gitea: fix
monochange_github: fix
monochange_gitlab: fix
monochange_go: fix
monochange_graph: fix
monochange_hosting: fix
monochange_npm: fix
monochange_python: fix
monochange_semver: fix
monochange_telemetry: fix
---

# Ship crate_docs.md in published crate tarballs

These crates embed their crate-level docs from `src/crate_docs.md` through `include_str!`, but their explicit `package.include` lists only covered `*.rs` files, so `cargo package` built tarballs whose `lib.rs` referenced a missing file. The 0.11.0 crates.io rollout failed at `monochange_core` with `couldn't read src/crate_docs.md` and every crate ordered after it went unpublished; a `cargo publish` of any of these crates failed deterministically.

Each manifest now includes the file:

```toml
include = ["src/**/*.rs", "Cargo.toml", "src/crate_docs.md", "readme.md"]
```

A `packaging_manifests` integration test fails CI when a crate embeds `src/crate_docs.md` without shipping it, so this class of publish failure cannot land again.
