---
"monochange_extension": feat
---

# add leptoskit browser extension scaffold

Maintainers can develop a Chromium/Firefox extension that signs in and glances at projects and website release status against the app backend.

- **`extension/` workspace** — `leptoskit.toml` with popup `/`, options `/sign-in`, and a background worker; build with `leptoskit build --platform extension`.
- **`monochange_extension_shared`** — stub RPC types and `HttpRpcClient` matching `SessionUser`, organisations/projects, and website releases (`/rpc/*`) until the web/RPC port publishes a shared crate.
- **Release floor chrome** — popup CSS publishes `--mc-*` tokens (teal accent, mist page ground) from the landing design system.
