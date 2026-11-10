---
monochange_extension: major
---

# Use shared sessions and product contracts in the extension

> Replace the extension's demo RPC contracts and manually stored bearer with the shared product client and interactive sign-in.

`monochange_extension_shared::{RpcClient, HttpRpcClient, HttpTransport}` and the `/rpc/*` endpoint constants are removed. Use `monochange_shared::ApiClient<T: Transport>` and the real `/api` server-function routes. Restore a saved `ClientSession` with `ApiClient::with_bearer(transport, origin, session.access_token)`; `session_user()`, `organizations()`, and `project(org, slug)` return the website's DTOs. Repository fields now follow `monochange_shared::RepoInfo`, including `github_full_name` and `github_private`.

```rust
// Before: extension-specific demo RPC client.
use monochange_extension_shared::HttpRpcClient;

// After: shared product client with a restored session.
use monochange_shared::ApiClient;
let api = ApiClient::with_bearer(transport, origin, session.access_token);
let organizations = api.organizations().await?;
```

The worker owns browser identity, single-use PKCE exchange, and `storage.local`. Users sign in from the popup instead of pasting a token. Register the installation's exact `identity.getRedirectURL()` in `MONOCHANGE_EXTENSION_REDIRECT_URIS` before enabling login; no production login is claimed without this registration.
