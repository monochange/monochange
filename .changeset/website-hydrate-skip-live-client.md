---
monochange_app: minor
---

# Expose a `_bg.wasm` pkg alias and stop unused live hydrate traffic

`public_routes::pkg_wasm_bg_alias` is a new public helper that maps cargo-leptos's on-disk `pkg/<name>.wasm` to the `pkg/<name>_bg.wasm` URL HydrationScripts may emit when `LEPTOS_OUTPUT_NAME` was unset while compiling `leptos`. Production `main` mounts it, and website hydrate no longer calls `leptoskit_rpc::client::configure`, so Lighthouse stops failing on hundreds of console 404s after the leptoskit SSR wave.

`configure` always starts a `LiveClient` that reconnects to `/live` (then `/live/events`). This website only exposes product RPCs as Leptos `#[server]` / `#[api]` over `POST /api/<endpoint>` and never mounts the live transport, so those reconnects were pure console noise. Product server functions are unchanged.

```rust
use leptos::prelude::LeptosOptions;
use monochange_app::public_routes::pkg_wasm_bg_alias;

let options = /* LeptosOptions from get_configuration */;
let (route, file) = pkg_wasm_bg_alias(&options);
// route == "/pkg/monochange_app_bg.wasm"
// file serves site_root/pkg/monochange_app.wasm
Router::new().route_service(&route, file);
```

```text
# before (404 when HydrationScripts asked for _bg)
GET /pkg/monochange_app_bg.wasm → 404

# after (same bytes as the cargo-leptos output)
GET /pkg/monochange_app_bg.wasm → 200  (serves pkg/monochange_app.wasm)
```
