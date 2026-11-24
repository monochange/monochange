---
monochange_app: feat
monochange_extension: fix
---

# Serve client sign-in as leptoskit RPC

Native guests and the browser extension now exchange their one-time connection code and sign out through leptoskit RPC operations, beside the product reads the shared `ApiClient` already calls as server functions:

| Operation               | Route                             | Arguments                         | Result          |
| ----------------------- | --------------------------------- | --------------------------------- | --------------- |
| `client_session_token`  | `POST /api/client_session_token`  | `{"request": ClientTokenRequest}` | `ClientSession` |
| `client_session_revoke` | `POST /api/client_session_revoke` | `{}` with the session's bearer    | `null`          |

Both run the REST routes' cores, `monochange_app_api::client_auth::exchange_code` and `revoke_bearer`, so the code is still burned before the verifier, client, and redirect are checked, a web `Origin` is refused without burning it, and a repeated revocation succeeds. A new `handoff_gate` keeps every answer `no-store` and the 4 KiB body limit; failures answer with the handoff statuses and a `{"status": 400, "error": "invalid handoff request"}` body. `HandoffError::status` and `HandoffError::message` are now public.

`monochange_shared::ApiClient::exchange` and `revoke` call the RPC operations and fall back to `POST /api/client-sessions/token` and `/revoke` when a site answers `404`, so new clients keep working against a site deployed before this change. The REST routes stay for clients released before it.
