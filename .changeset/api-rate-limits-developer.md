---
monochange_app: feat
---

# Rate limit the API per client

The new `monochange_app::rate_limit` module limits Leptos server functions and the REST endpoints under `/api` with [`governor`](https://docs.rs/governor) GCRA buckets. Requests with a valid session cookie count against the signed-in user's bucket (300 a minute, burst 100); every other request counts against its client address (60 a minute, burst 30), with IPv6 addresses grouped by their /64. Pages, `/pkg` assets, `/health`, and `/api/github/webhooks` are not limited.

`X-Forwarded-For` is read from the right and only when the peer is a loopback or private address, which is Caddy in the production deployment. The server now runs with `into_make_service_with_connect_info::<SocketAddr>()` so the middleware sees the peer address.

Limited responses carry `RateLimit-Policy` and `RateLimit` fields, and a `429 Too Many Requests` adds `Retry-After`. Server function rejections use the `ServerFnError` encoding (`ServerError|Too many requests. Try again in 12 seconds.`); REST rejections use the endpoints' `{"message": ...}` JSON body.

Override the limits with environment variables; a value that isn't a whole number above zero stops startup:

```bash
MONOCHANGE_RATE_LIMIT_ANONYMOUS_PER_MINUTE=60
MONOCHANGE_RATE_LIMIT_ANONYMOUS_BURST=30
MONOCHANGE_RATE_LIMIT_AUTHENTICATED_PER_MINUTE=300
MONOCHANGE_RATE_LIMIT_AUTHENTICATED_BURST=100
```
