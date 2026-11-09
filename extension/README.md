# monochange projects extension

The popup reads the shared product API. Its background worker owns interactive browser sign-in, the PKCE exchange, and the device credential in `storage.local`. Closing the popup does not move the verifier or exchange into a short-lived page. Sign-out revokes the device session and clears its local credential. The website's cookie remains on the website.

Before enabling login, load the packaged extension in an isolated browser profile and inspect `identity.getRedirectURL()` from its worker. Register that exact HTTPS callback in the backend's `MONOCHANGE_EXTENSION_REDIRECT_URIS` configuration. The configured Firefox ID is `monochange@monochange.dev`. Chromium unpacked IDs can differ between installations; register the actual generated callback rather than guessing an ID. Production login remains unavailable until this registration is complete.

`[extension].server` in `leptoskit.toml` supplies the API origin and its generated host permission. Keep it exact when building for another backend. No arbitrary host permission or pasted bearer token is required. Build and load both Chromium and Firefox bundles before claiming cross-browser runtime verification.
