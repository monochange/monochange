---
monochange_app: fix
---

# Bound OIDC signing key refreshes independently of token key IDs

Authentication for `POST /api/release-commits` and `POST /api/release-requests` now rejects unknown signing key IDs from a fresh JWKS cache without contacting GitHub. The cache remains valid for ten minutes. Concurrent refreshes share one request, and a shared retry deadline of thirty seconds prevents failed or cancelled attempts from restarting once per key ID. Invalid tokens can no longer force one provider request per attempt.

JWKS responses must have a successful HTTP status and fit within 64 KiB before parsing. Failed refreshes retain the previous key set but do not authorize tokens with expired keys. Deployments and workflow callers need no configuration changes.
