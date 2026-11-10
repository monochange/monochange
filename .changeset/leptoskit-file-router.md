---
monochange_app: major
---

# Serve the public site from the leptoskit file router

> **Breaking change:** `HomePage`, the Leptos route structs, and the projects page reexports are gone.
>
> Replace `pages::home::HomePage` with `pages::home::home_screen` (route `/`). Replace `pages::dashboard::DashboardRoute`, `pages::install::InstallRoute`, and `pages::pricing::PricingRoute` with the file-router modules under `routes::` (`/dashboard`, `/install`, `/pricing`). Import `monochange_projects::pages::{feedback_console, organization, portal, project}` when you still need those components; `monochange_app::pages` now has its own modules under those names. `pages::dashboard::OrganizationAvatar` still takes `login` and `avatar_url`, and it renders the leptoskit avatar instead of `<img class="organization-avatar">`.

```rust
// Before
use monochange_app::pages::dashboard::DashboardRoute;
use monochange_app::pages::home::HomePage;
// After
use monochange_app::pages::home::home_screen;
use monochange_projects::pages::feedback_console;
use monochange_projects::pages::organization;
use monochange_projects::pages::portal;
use monochange_projects::pages::project;
```

Public pages now come from `src/routes/` instead of `leptos_router` `<Routes>`. The pinned leptoskit revision's `navigator()` supplies the marketing tab bar. Screens are `view!` markup, which is what `markup_style = "deny"` accepts. SSR still streams one document: known pages are explicit GET routes, and server functions stay on their existing paths. `/dashboard/...` project links redirect to `/projects/...`, which uses the projects shell. Patch coverage compiles `monochange_app` with the `ssr` feature so the server renderer is in the report.

The release client package no longer emits per-route wasm-split chunks, so `monochange_app.wasm` holds that code (gzip 598287 to 1036900 bytes) and `monochange_app.js` grows from 7821 to 8751 gzip bytes. Total gzip transfer drops from 1513294 to 1067799 bytes because the old chunks are gone. `scripts/website-size-baseline.json` records that package.
