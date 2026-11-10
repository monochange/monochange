---
monochange_app: feat
---

# Serve the public site from the leptoskit file router

Public pages now come from `src/routes/` and the leptoskit 0.3 stack (`navigate`, `back`, `link`) instead of `leptos_router` `<Routes>`. SSR still streams one document: known pages are explicit GET routes, server functions stay on their existing paths, and the native guest uses the same file-router layout with an in-memory stack. Published 0.3 has no Stack, Tabs, or Drawer `navigator()`, so the tab row is built from `Router::link`.
