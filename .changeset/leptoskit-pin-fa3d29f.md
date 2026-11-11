---
monochange_app: fix
monochange_extension: fix
---

# Move the leptoskit pin to the 2026-11-10 upstream revision

The web app, native workspace, browser extension, and the managed `leptoskit` CLI now build against one reviewed upstream commit, `fa3d29f8e8c837b19f92f1e9a95f699803543767` (upstream `main` captured on 2026-11-10), instead of the 2026-11-09 revision. Every direct dependency in the three workspaces and the root CLI pin name the same immutable full Git revision, and all three lockfiles resolve it, so `cargo generate-lockfile` retains the source. Before:

```toml
# Cargo.toml [workspace.metadata.bin]
leptoskit_cli = { version = "0.3.0", git = "https://github.com/leptoskit/leptoskit", rev = "a93214edc8e5f6c7999c628285093f15cf188639", locked = true, bins = ["leptoskit"] }
```

After:

```toml
# Cargo.toml [workspace.metadata.bin]
leptoskit_cli = { version = "0.3.0", git = "https://github.com/leptoskit/leptoskit", rev = "fa3d29f8e8c837b19f92f1e9a95f699803543767", locked = true, bins = ["leptoskit"] }
```

Reinstalling the lint CLI keeps the same provenance guard as before; only the revision moves:

```bash
cargo install --git https://github.com/leptoskit/leptoskit \
  --rev fa3d29f8e8c837b19f92f1e9a95f699803543767 --locked --force leptoskit_cli
```

The native workspace now pins `wasm-bindgen` to `=0.2.129` in `monochange_native_app`, matching the pins the web app and extension already carry. The pinned CLI embeds `wasm-bindgen-cli-support` 0.2.129, so a natively resolved 0.2.130 family would make the CLI reject the produced wasm. The native lock resolves the whole family at 0.2.129 with its paired `js-sys`/`web-sys` 0.3.106.
