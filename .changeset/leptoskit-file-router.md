---
monochange_app: feat
---

# Serve the public site from the leptoskit file router

Public pages now come from `src/routes/` and leptoskit git `fa3d29f` instead of `leptos_router` `<Routes>`. That revision's `navigator()` supplies the tab bar. Screens are `view!` markup, which is what `markup_style = "deny"` accepts. SSR still streams one document: known pages are explicit GET routes, and server functions stay on their existing paths. The website, native guest, and extension all pin that same commit. Patch coverage compiles `monochange_app` with the `ssr` feature so the server renderer is in the report.
