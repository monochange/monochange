---
monochange_app: fix
---

# Defer public document hydration and preserve button clicks

The server selects deferred hydration for the public website routes and the signed-out projects landing page. The loader starts on interaction or an idle fallback after load, queues clicks on buttons outside forms, and awaits the browser entry's async hydration export before replaying those clicks in order. Links, native details, and form submissions retain their browser defaults. Signed-in projects and editable product pages continue hydrating eagerly.

Deploy the server and both browser entries together: the loader now awaits `hydrate_from_async()` so lazy routes attach before replay. Deferred documents omit entry and lazy-chunk preloads to keep WASM work off the initial rendering path.
