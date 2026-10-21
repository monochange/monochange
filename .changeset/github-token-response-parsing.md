---
monochange_app: fix
---

# Read GitHub OAuth token replies without discarding the error

`exchange_code` and the user-token refresh in `github_user_access_token` no longer call `reqwest::Response::json` on `POST /login/oauth/access_token`. They read the body as text and parse it in `parse_token_response`, which:

- returns `GitHub rejected the sign-in: <error_description> (<error>)` when GitHub sends an OAuth error instead of a token;
- accepts `application/x-www-form-urlencoded` replies and lifetimes sent as strings (`"expires_in": "28800"`);
- reports any other body as `GitHub returned an unreadable token response (HTTP <status>, <content type>): <title or first 200 characters>` and logs it with `tracing::warn!`.

Refresh failures still ask the user to sign in again, but the server log now carries GitHub's reason.
