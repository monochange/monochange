---
monochange_app: website_feature
---

# Fair use limits for the monochange.dev API

monochange.dev now limits how quickly one client can call its API, so a single script can't slow the site down for everyone else. Normal browsing never comes close to the limit. Signed-in users get five times the allowance of anonymous visitors.

If you do hit the limit, the site tells you how many seconds to wait before trying again. Scripts that read the public task API get a `429 Too Many Requests` response with a `Retry-After` header, and every API response reports how many requests remain.
