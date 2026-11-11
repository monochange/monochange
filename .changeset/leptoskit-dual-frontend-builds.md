---
monochange_app: major
---

# Build each web frontend with the pinned leptoskit CLI and assemble one site

The website and projects browser entries now build through their own `leptoskit.toml` with the provenance-verified `leptoskit` CLI, replacing the cargo-leptos pipeline in CI and the deployment image. Both configs name the shared `monochange_app` backend and select their own browser package, and `leptoskit.workspace.toml` at the repository root declares website (landing), projects (product), native (product), and extension (product).

The projects browser package is the dedicated `monochange_projects_web` cdylib, which owns the product route tree (`#[lazy_route]` pages) and the wasm-bindgen `hydrate` export; `monochange_projects` stays the shared product library (components, server functions, embedded feedback) with no cdylib and no `web`/`native` feature pair. The split is load-bearing for size, not naming: `#[lazy_route]` emits `#[no_mangle]` wasm-split exports in the crate that declares them, and those exports are linker roots in every browser bundle that compiles the crate. With the routes in the shared library, the website bundle retained the whole product route code (~824 KiB raw) as unreachable-but-exported functions; with the dedicated entry crate, the website client carries none of it. The website backend still renders the product route tree server-side through a host-only dependency on the entry crate, and the projects bundle loads `/pkg/projects/monochange_projects_web.js`.

Before (CI and Docker):

```bash
cargo leptos --manifest-path crates/monochange_app/Cargo.toml build --release --split
pnpm node scripts/minify-split-loader.ts --site-root app/target/site
find /site/pkg -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.css' \) \
  -exec sh -c 'gzip -9 -k "$1" && brotli -q 11 -c "$1" > "$1.br"' _ {} \;
```

After:

```bash
# resolves the CLI from [workspace.metadata.bin] and verifies the Git pin
leptoskit_bin="$(scripts/leptoskit-cli.sh)"
(cd app && "$leptoskit_bin" build --release --platform web --out target/web)
(cd app/projects && "$leptoskit_bin" build --release --platform web --out ../target/site)
```

The tooling plugin declared under `[[plugins]]` in both web configs runs inside each build, before release precompression: it compiles the shared Tailwind input to the entry CSS (the leptoskit build produced no CSS on its own), minifies the wasm-split loader and each entry's wasm-bindgen glue through the guarded `scripts/minify-split-loader.ts` (restoring cargo-leptos `js-minify` parity for the main `{output_name}.js`), and places each frontend's bundle under `pkg/site/` or `pkg/projects/` so both entries assemble into one deployment without colliding split manifests or chunk names. Missing tools fail the build loudly; run `leptoskit doctor` to check a build environment. The plugin resolves as `leptoskit-plugin-monochange-web-assets` on PATH (the devenv profile provides it; provide the same symlink in other environments).

The server composes both SSR route sets in one registration — `leptos_routes_with_context` receives the merged route listings of both trees, so `/api` server functions register exactly once — and renders the entry scripts, stylesheet, and split manifest of the frontend that owns the request path. leptos 0.9's `HydrationScripts` caches entry names process-globally, so the composition renders per-frontend hydration scripts explicitly; the emitted markup matches upstream for each entry. Because the router intercepts same-origin anchor clicks, links that cross between the website and projects entries render with `target="_self"`, forcing the real document navigation that loads the other bundle.

The committed size gate (`scripts/website-size.ts check`) still compares against the pre-split baseline: per-file guards follow assets across the namespace move (`pkg/` → `pkg/site/`, `pkg/projects/`), the cargo-leptos→wasm-bindgen entry rename (`monochange_app.wasm` → `site/monochange_app_bg.wasm`), the `auth_callback`→`site_session` route rename, and cross-namespace lazy-route hashes, refusing ambiguous or vanished matches instead of skipping them, and both namespaces stay inside the unchanged total. The raw benchmark budgets (`scripts/benchmark-website.ts`) no longer count precompressed `.br`/`.gz` twins, and every frontend's eager entry wasm is checked against the eager budget by name.

The Docker image builds the pinned CLI from the same Git revision instead of installing a release archive, installs binaryen for `wasm-opt`, and keeps the Monosecret privilege drop, no-new-privileges, SQLite volume, secret mounts, `/health`, and Class B APIs unchanged. `scripts/lint-leptoskit.sh` now resolves the CLI through the shared `scripts/leptoskit-cli.sh` helper, so lint, build, and workspace commands use one pin implementation.
