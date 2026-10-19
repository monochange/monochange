---
monochange_app: website_feature
---

# Faster pages and smaller downloads on monochange.dev

The website now loads its interactivity in per-route pieces instead of one large download: the home page no longer fetches the code behind the dashboard, book, changelog, pricing, or feedback portal, and each of those pages fetches its code only when you open it. Transfers are compressed and reusable assets are cached with cheap revalidation, so returning visits re-download almost nothing and pages after the first feel immediate, especially on slower connections.
