---
monochange_app: fix
---

# Resolve the hosted release API token through SecretSpec

The hosted release endpoints (`POST /api/release-commits` and `POST /api/release-requests`) read `MONOCHANGE_TOKEN` straight from the process environment, so it bypassed SecretSpec and was the only release secret that the configured provider (1Password in production) never supplied.

`MONOCHANGE_TOKEN` is now an optional password entry in `app/secretspec.toml` for the `default` and `production` profiles and is loaded into `AppState::api_token`; an empty value counts as unset. Behavior is unchanged: tokens are compared in constant time, an unconfigured token answers `503`, and a wrong token answers `401`.

Operators who set `MONOCHANGE_TOKEN` as a plain environment variable on the server must store it as `secretspec/monochange_app/production/MONOCHANGE_TOKEN` in the production vault instead. Deployments that rely only on GitHub Actions OIDC need no change.
