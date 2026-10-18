---
monochange_app: feat
---

# Require GitHub sign-in to write on the feedback portal

`share_feedback`, `portal_vote`, and `portal_reply` now refuse a visitor without a valid session ("Sign in with GitHub to share.", "… to vote.", "… to join the discussion."). `portal_view` stays public: a missing, expired, or forged session reads as signed out, and `PortalSnapshot` gains `viewer: Option<PortalViewer>` with the visitor's login and avatar.

A signed-in visitor's per-project id comes from the new `monochange_app_api::feedback::account_viewer_id(secret, provider, external_id, project_id)`, an HMAC of `"{provider}:{external_id}"`. One account votes once per project, ids can't be linked across projects, and stored feedback never names a GitHub login. The `__Host-monochange_viewer` cookie and `portal::VIEWER_COOKIE` are removed. Items shared under a cookie id keep their votes and stay listed, but their author no longer sees them under "Yours".

Sign-in can return to where it started. `get_login_url(next: Option<String>)` and `oauth::login_state_returning(secret, next)` sign the destination into the OAuth state cookie, `VerifiedOAuth::return_to` carries it back, and `exchange_code` redirects there instead of `/dashboard`. `oauth::return_path` accepts only same-site paths, so absolute, protocol-relative, and backslashed values, control characters, and paths over 512 bytes fall back to the dashboard:

```text
/login?next=%2Fp%2Facme%2Fpocketbook%23roadmap  → back to /p/acme/pocketbook#roadmap
/login?next=%2F%2Fevil.example                  → /dashboard
```
