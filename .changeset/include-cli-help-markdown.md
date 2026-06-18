---
monochange: patch
monochange_core: patch
monochange_publish: patch
---

# include CLI help markdown in published crate packages

The `monochange` crate package now includes markdown files from `src/` when Cargo builds the publish tarball. This keeps embedded CLI help text available to downstream builds and to `cargo publish` verification.

Before this fix, publishing could package `src/cli.rs` without the `src/cli_after_long_help.md` file referenced by `include_str!`, causing the crate verification step to fail with a missing-file error.

Command:

```bash
cargo publish --dry-run --manifest-path crates/monochange/Cargo.toml
```

After this change, the dry run can compile the packaged tarball because `src/cli_after_long_help.md` is included alongside the Rust sources.

The `monochange step publish-packages --stream-output` flag now streams package-manager stdout and stderr while commands run, while still capturing those streams in the publish report. The publish workflow enables this opt-in flag so Cargo verification errors from `cargo publish`, including missing packaged files, are visible in the normal CI log instead of only appearing in a later summarized report or annotation.
