---
monochange_app: fix
---

# Tighten hosted endpoint verification and startup secret checks

The hosted release API now rejects three edge cases that previously slipped through verification:

- GitHub Actions OIDC tokens must be signed with RS256. The verifier pinned the algorithm to the token header before, so a token that named any other algorithm selected how its own signature was checked.
- The server refuses to start when the resolved `JWT_SECRET` is an empty string. An empty secret is a valid HMAC key, and a profile that resolves the secret to nothing would have signed session tokens anyone could forge.
- The unreachable "repository mismatch" check after the connected-repository lookup is gone. Repositories are resolved by exact full name, so the check could never fire.

The constant-time comparison used for `MONOCHANGE_TOKEN` checks is now shared with the webhook signature path instead of existing as two identical copies.
