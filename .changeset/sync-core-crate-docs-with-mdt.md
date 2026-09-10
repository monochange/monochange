---
monochange_core: fix
---

# Sync monochange_core crate docs with mdt instead of include_str!

`monochange_core` embedded its crate-level docs from `src/crate_docs.md` through `include_str!`, so `cargo publish` failed whenever the markdown file was missing from the package tarball — the failure mode that aborted the 0.11.0 crates.io rollout at `monochange_core`. The crate-level docs now live in an mdt consumer block directly inside `src/lib.rs`, and `package.include` no longer lists `src/crate_docs.md`, so the published tarball compiles without a separate markdown file. Rustdoc output is unchanged apart from removing a duplicated example section and stray fragments that trailed the crate docs.
