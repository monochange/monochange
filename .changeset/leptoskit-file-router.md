---
monochange_app: feat
---

# Serve the public site from the leptoskit file router

Public pages now come from `src/routes/` instead of `leptos_router` `<Routes>`. The pinned leptoskit revision's `navigator()` supplies the marketing tab bar. Screens are `view!` markup, which is what `markup_style = "deny"` accepts. SSR still streams one document: known pages are explicit GET routes, and server functions stay on their existing paths. `/dashboard/...` project links redirect to `/projects/...`, which uses the projects shell. Patch coverage compiles `monochange_app` with the `ssr` feature so the server renderer is in the report.
