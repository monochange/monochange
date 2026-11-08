---
monochange_app: breaking
---

# Upgrade the website to Leptos 0.9 with leptoskit_rpc

The nested `app/` workspace now links **Leptos 0.9.0-rc1** and **leptoskit / leptoskit_rpc 0.3**. Product RPCs register through `#[api]` on stable `/api/<endpoint>` paths, while cookie auth and Class B Axum mounts stay on hand-rolled routers.

Wire DTOs that used to live only under `monochange_app::server_fns::*` now define in `monochange_shared` and are re-exported from the old modules. Prefer the shared crate for new guests (extension / native); existing `server_fns` imports keep compiling through the re-exports, but public-api tooling treats the move as a major surface change.

```rust
// Prefer shared DTOs for new callers:
// Existing app code can keep:
use monochange_app::server_fns::auth::SessionUser;
use monochange_shared::OrganizationSummary;
use monochange_shared::SessionUser;
use monochange_shared::WebsiteRelease;
```

Form actions and server-fn clients must post to the stable endpoint path (no build-hash suffix):

```text
# before (Leptos 0.8 hashed server-fn URL)
POST /api/get_session/<build-hash>

# after (leptoskit_rpc #[api] + endpoint = "…")
POST /api/get_session
```

- **Leptos 0.9 stack** — Workspace `leptos`, `leptos_axum`, `leptos_meta`, `leptos_router`, and `server_fn` move to `0.9.0-rc1`. App `ssr` enables `leptoskit/server` + `leptoskit/rpc-ssr`; `hydrate` enables `leptoskit_rpc/web`.
- **`#[api]` on product RPCs** — All 31 product `#[server]` functions take `#[api(crate = "leptoskit_rpc", tag = "…")]` with `endpoint = "<name>"`.
- **Server / hydrate wiring** — `main` uses `leptoskit::server` re-exports; hydrate calls `leptoskit_rpc::client::configure` then `hydrate_lazy`. Class B routes still mount via `monochange_backend::machine_api_router`.
- **0.9 adaptation** — Rate-limit server-fn errors use `ServerFnError::ser()` response parts; OAuth fallback alias tests send `Host` so SSR path normalization matches `rate_limit::surface`.
