---
monochange_app: breaking
---

# Replace page components with lazy route types

The public page components (`BookPageView`, `ChangelogPage`, `DashboardPage`, `FeedbackConsolePage`, `InstallPage`, `OrganizationPage`, `PortalPage`, `PricingPage`, `ProjectPage`, and the private `AuthCallbackPage`) are gone. Each route is now a `LazyRoute` implementation (`BookRoute`, `ChangelogRoute`, `DashboardRoute`, `FeedbackConsoleRoute`, `InstallRoute`, `OrganizationRoute`, `PortalRoute`, `PricingRoute`, `ProjectRoute`, `AuthCallbackRoute`) whose `data()` creates the route's resources and whose `view()` renders them, referenced from `app.rs` through `Lazy::<Route>::new()`. Nothing outside this crate consumed the components; the website's rendered output is unchanged.
