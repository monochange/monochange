---
monochange_app: fix
---

# Minify the wasm-split loader in release builds

The website release build now minifies `pkg/__wasm_split.*.js` — the loader module that maps every lazy route to its WASM chunks — before the site is packaged. cargo-leptos `js-minify` covers the main `monochange_app.js` glue only, so the loader kept its per-chunk route comments and full identifiers and failed the committed bundle-size gate (2,234 gzip bytes versus the 1,789-byte baseline, above the 5% per-file allowance). After minification the loader ships at 1,484 gzip bytes (6.9 KB raw, down from 26.3 KB), within every gate with no baseline, tolerance, or exclusion change.

The step is part of the release pipeline, not a manual chore: app CI runs it between `cargo leptos build --release --split` and the size/Lighthouse checks, and the Docker release builder runs it before the site is copied out and precompressed, so the deployed gzip/brotli siblings always match the gate-measured bytes. The upstream `leptoskit build --release` web build emits the same loader shape; pass its output directory to the script's `--site-root` when building that way:

```bash
cargo leptos --manifest-path app/crates/monochange_app/Cargo.toml build --release --split
pnpm node scripts/minify-split-loader.ts --site-root app/target/site
```

The workspace permits the install script for `esbuild@0.28.2` through pnpm's `allowBuilds` setting so strict CI installs can provision its native binary.

The rewrite is guarded: `scripts/minify-split-loader.ts` replaces a loader only when the minified output still exports the same `__wasm_split_load___<route>` initializers and still references the same `/pkg/monochange_app.js` import and `./chunk_N.wasm` / `./split___<route>.wasm` URLs. A minifier regression or an unexpected loader shape fails the build instead of reaching a deployment. esbuild is resolved from `MONOCHANGE_ESBUILD_BIN`, the workspace `node_modules/.bin/esbuild` devDependency, or `PATH`, in that order; the Docker builder installs a pinned esbuild binary (registry integrity digest verified) plus Node 24 so the identical script runs there.
