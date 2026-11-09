---
monochange_app: minor
---

# Add the shared client-session handoff and one authorization resolver

Every authorization path in the web app now funnels through one resolver (`monochange_app_api::sessions`), which distinguishes a browser cookie principal from an opaque client-token principal and never falls back to a cookie when an `Authorization` header is present.

New additive schema in `monochange_app_db`: `users.session_epoch` (default 0) plus `client_grants` (hashed one-time codes bound to user, client, exact redirect, PKCE challenge, and epoch) and `client_sessions` (hashed opaque `mcd_` tokens). No secrets are stored, only SHA-256 hashes; codes are consumed by an atomic `DELETE ... RETURNING`, so a wrong verifier, client, or redirect still burns the code.

Browser handoff: `POST /connect/approve` accepts a browser-only, same-origin form post, validates the exact registered redirect and PKCE input, and 303-redirects with only an opaque code and state. `POST /api/client-sessions/token` exchanges code + verifier for a scoped client session; `POST /api/client-sessions/revoke` self-revokes idempotently. All handoff responses, including failures, are `Cache-Control: no-store` with a 4 KiB body cap.

New browser-only server functions: `connect_request` (consent preview), `list_client_sessions`, `revoke_client_session`, and `sign_out_everywhere`, the last of which increments the user's epoch, deletes all their grants and client sessions, and clears the host-only cookie. State-changing session calls require the site's own `Origin`.

Device clients (native/extension) may call `get_session`, `list_organizations`, `organization_overview`, `project_overview`, and `get_website_releases` with their bearer token, and revoke themselves; every other server function is browser-only. Existing ordinary logout behavior is unchanged; sign-in now mints the browser JWT at the user's current epoch. JWTs issued before this change keep working until the first `sign_out_everywhere` bumps the epoch.

Redirect allowlists are plain public environment config resolved once at startup, following the existing `AppState` pattern: `MONOCHANGE_NATIVE_REDIRECT_URIS` (default `dev.monochange.app:/auth/native/return`, validated by the upstream `leptoskit_plugin_web_auth` return-address helper) and `MONOCHANGE_EXTENSION_REDIRECT_URIS` (empty by default, so no extension can connect until its exact `https` redirect is configured).
