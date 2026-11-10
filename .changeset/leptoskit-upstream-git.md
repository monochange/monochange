---
monochange_app: fix
monochange_extension: fix
---

# Resolve leptoskit from the upstream Git revision

The web app, native workspace, and browser extension now build against one reviewed upstream `leptoskit` commit instead of separate crates.io 0.3.0 copies. This keeps every surface on the same source and lets `cargo update` pick up upstream fixes without waiting for a registry release.

`lint:leptoskit` now reads the same pinned revision from `[workspace.metadata.bin]`, verifies every `leptoskit*` lockfile source against it, and force-installs the CLI when the cached binary came from crates.io or another revision. A broad `.bin` cache restore can no longer leave the lint task running a stale same-version binary:

```bash
devenv shell lint:leptoskit
# manual equivalent:
cargo install --git https://github.com/leptoskit/leptoskit \
  --rev a93214edc8e5f6c7999c628285093f15cf188639 --locked --force leptoskit_cli
```

The web app additionally pins `wasm-bindgen` to `=0.2.129` (with its `js-sys`/`web-sys` family resolving to 0.3.106) to match the bindgen ABI the pinned CLI generates against. Floating `0.2` resolved 0.2.130, whose generated JS glue imports a `__wbindgen_externref_xform__` module specifier the served bundle does not provide, so every route failed hydration in the browser with `TypeError: Failed to resolve module specifier "__wbindgen_externref_xform__"` while the build and size budgets still passed. The extension already carried the same pin.
