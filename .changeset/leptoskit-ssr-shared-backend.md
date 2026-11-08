---
monochange_app: feat
---

# Start the leptoskit SSR layout with shared DTOs and Class B mounts

The website workspace gains the first leptoskit-shaped scaffolding without changing cookie auth or Class B machine HTTP contracts.

- **`app/leptoskit.toml`** — SSR on port 3000, `client_package = "monochange_app"`, route-split WASM bundles. Prefer `leptoskit serve` / `build` when the CLI is installed; `cargo leptos` remains the documented fallback.
- **`monochange_shared`** — Wire DTOs (`SessionUser`, repo/org/project summaries, installation status, book and website release types) for the WASM UI and future extension/native guests. Existing `server_fns` modules re-export them so imports stay stable.
- **`monochange_backend`** — `machine_api_router` mounts GitHub webhooks, hosted release OIDC, embed SDK, and CLI task routes via `monochange_app_api::api_router`.
- **Stable RPC paths** — Product `#[server]` functions set `endpoint = "<name>"` so leptoskit_rpc `#[api]` can pin routes after the Leptos 0.9 bump. Workspace deps declare `leptoskit` / `leptoskit_rpc` 0.3; the app still compiles on Leptos 0.8 until that upgrade lands.
