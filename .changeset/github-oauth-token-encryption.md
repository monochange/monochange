---
monochange_app: fix
---

# Encrypt GitHub OAuth tokens at rest and scope the hosted API token

Two changes to the monochange.dev database and hosted API:

- GitHub user access and refresh tokens are now sealed with AES-256-GCM before they are written to the SQLite database, under a new required `TOKEN_ENCRYPTION_KEY` secret (`auth` section for production, generated in development and CI). A database file disclosure no longer leaks live GitHub tokens. Rows written before this change stay readable in plaintext and are replaced with sealed values the next time the token refreshes; rotating the key invalidates stored tokens and users simply sign in again.
- The deployment-wide `MONOCHANGE_TOKEN` no longer authorizes every connected repository. It only acts on repositories listed in the new `MONOCHANGE_TOKEN_REPOSITORIES` environment variable (`owner/repo`, comma or whitespace separated, case-insensitive). Deployments that rely on the token must list the repositories it may act on, or use GitHub Actions OIDC tokens, which were already restricted per repository.
