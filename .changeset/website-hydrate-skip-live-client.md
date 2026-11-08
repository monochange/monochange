---
monochange_app: fix
---

# Stop hydrate from requesting missing live and `_bg` assets

Website hydrate no longer calls `leptoskit_rpc::client::configure`, and the production server now serves cargo-leptos's on-disk `pkg/<name>.wasm` at the `pkg/<name>_bg.wasm` URL. Together these stop the app-ci Lighthouse console gate from failing on hundreds of 404s after the leptoskit SSR wave.

`configure` always starts a `LiveClient` that reconnects to `/live` (then `/live/events`). This website only exposes product RPCs as Leptos `#[server]` / `#[api]` over `POST /api/<endpoint>` and never mounts the live transport, so those reconnects were pure console noise. Product server functions are unchanged.

HydrationScripts appends `_bg` to the wasm filename when `LEPTOS_OUTPUT_NAME` was unset while compiling `leptos` (common with shared CI/dep caches). cargo-leptos still writes `<output>.wasm` only. Production now remaps the `_bg` URL the same way the ignored Chrome fixture has since #761:

```text
# before (404 when HydrationScripts asked for _bg)
GET /pkg/monochange_app_bg.wasm → 404

# after (same bytes as the cargo-leptos output)
GET /pkg/monochange_app_bg.wasm → 200  (serves pkg/monochange_app.wasm)
```
