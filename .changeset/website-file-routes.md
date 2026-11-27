---
monochange_app: refactor
---

# Register the website from the pinned file routes

`monochange_app` now enables the pinned leptoskit `router` plugin for the website. `src/routes/` is the source of truth and the committed `src/routes.rs` supplies the typed `Route` enum, hrefs, recognition, and the website HTTP paths the server registers. Registration goes through the toolkit's SSR listing helper, while each generated path reuses the render metadata discovered from the Leptos tree so Suspense streaming and lazy-route preloads are unchanged. Projects listings and the single server-function registration stay as they were.

The pinned generator writes a trailing catch-all as `/{*name}` without its static prefix, so the book route expands narrowly to `/book` and `/book/{*chapter}`; the raw `/{*chapter}` would swallow `/pkg` and the malformed router aliases. The Leptos route tree still renders every screen, because the generated native renderer cannot carry SSR resources, named forms, or per-route wasm chunks.
