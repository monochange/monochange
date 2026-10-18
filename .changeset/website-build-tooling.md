---
monochange_app: feat
---

# Split the website client, compress the edge, and gate page performance

Release builds must now pass `cargo leptos build --release --split` (the Dockerfile and app CI do): every route except the landing and login views is a `#[lazy_route]` boundary, so the initial WASM download excludes dashboard, organisation, project, feedback, portal, book, changelog, install, and pricing code. Server functions referenced only by those views move into their route chunks, and the client hydrates through `leptos::mount::hydrate_lazy`, which lazy routes require.

Build and deployment changes:

- The WASM client compiles with the new `wasm-release` profile (`opt-level = "z"`, `panic = "abort"`, js-minify enabled) while the server binary stays on a runtime-first `release` profile (`opt-level = 3`).
- `strip` moved from the cargo profile into the Dockerfile: cargo's implicit `-C strip=debuginfo` corrupts proc-macro dylibs on newer macOS hosts, so profiles set `strip = "none"` and the image strips the server binary after linking. Proc-macros and build scripts build at `opt-level = 0` via `build-override`, which also speeds up cold builds.
- The Dockerfile uses BuildKit cache mounts for the cargo registry and target directory, so dependency artifacts survive between image builds.
- `/pkg` hydration assets are served `Cache-Control: public, max-age=0, must-revalidate` and the static file handler answers `If-Modified-Since` with 304s, so a deployment can never pair fresh HTML with stale client code while repeat visits revalidate for free. The image ships precompressed `.gz`/`.br` siblings of every pkg asset that the file handler serves to accepting clients, and the Caddy edge enables `encode zstd gzip` for everything else — production previously shipped every byte uncompressed.
- `app/deploy/verify-image.sh` also asserts the split build's manifest and that pkg assets are served gzip-compressed to accepting clients.

New website benchmarking, run by the `benchmark` job in `app-ci.yml` on every app-affecting change and locally through `pnpm bench:website` / `pnpm size:website`:

- `scripts/benchmark-website.ts` boots the release server against a throwaway SQLite database and runs Lighthouse (desktop preset) across the core flows, failing when metrics in `scripts/website-budgets.json` regress. `MONOCHANGE_ALLOW_BENCHMARK_BUDGET_FAILURES=true` keeps the CLI benchmark escape hatch.
- `scripts/website-size.ts` compares the gzip transfer size of `app/target/site/pkg` against the committed `scripts/website-size-baseline.json` and fails when the total grows beyond 1% (or a transferred file beyond 5%, absorbing cross-platform codegen variance) until the baseline is deliberately updated with `pnpm size:website:update`.
