---
monochange_app: feat
---

# Upgrade the website to Leptos 0.9 with leptoskit_rpc

The nested `app/` workspace now links **Leptos 0.9.0-rc1** and **leptoskit / leptoskit_rpc 0.3** so product RPCs register through `#[api]` while cookie auth and Class B Axum mounts stay unchanged.

- **Leptos 0.9 stack** — Workspace `leptos`, `leptos_axum`, `leptos_meta`, `leptos_router`, and `server_fn` move to `0.9.0-rc1`. App `ssr` enables `leptoskit/server` + `leptoskit/rpc-ssr`; `hydrate` enables `leptoskit_rpc/web`.
- **`#[api]` on product RPCs** — All 31 product `#[server]` functions take `#[api(crate = "leptoskit_rpc", tag = "…")]`. Form posts use stable `/api/<endpoint>` paths (no build-hash suffix).
- **Server / hydrate wiring** — `main` uses `leptoskit::server` re-exports; hydrate calls `leptoskit_rpc::client::configure` then `hydrate_lazy`. Class B routes still mount via `monochange_backend::machine_api_router`.
- **0.9 adaptation** — Rate-limit server-fn errors use `ServerFnError::ser()` response parts; OAuth fallback alias tests send `Host` so SSR path normalization matches `rate_limit::surface`.
