---
monochange_app: website_fix
---

# Show why GitHub sign-in failed

When GitHub refuses a sign-in, the error page now shows GitHub's own reason, such as an expired or already used authorization code, instead of "error decoding response body". If GitHub answers with something that is not a token at all, such as an outage or rate-limit page, the page names the HTTP status and the page title so you know whether to retry.
