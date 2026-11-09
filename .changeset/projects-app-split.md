---
monochange_app: major
---

Move the browser workspace into `monochange_projects` at `/projects`, with its own navigation. `/dashboard` retains GitHub repository connection; existing organization, project and feedback links redirect into projects. Mount `/connect` and `/account` for client consent and revocation against the existing cookie authority. `connect_request` returns the validated `denial_url`; approval forms submit only `client`, `redirect_uri`, `code_challenge`, and `state`.

Native and extension consumers use `monochange_shared::ApiClient` rather than demo RPC endpoints. Native plugin declarations add crypto, web-auth, and linking alongside HTTP and secure-store: rebuild native hosts before distributing this update. This requires a new native binary, not an OTA-only update.

Register the exact URL returned by the installed extension's `identity.getRedirectURL()` in `MONOCHANGE_EXTENSION_REDIRECT_URIS` before testing or distributing extension login. The Firefox ID remains configured; Chromium unpacked IDs depend on the installation. There is no wildcard callback registration.
