---
monochange_app: fix
---

# Resolve the hosted release API token through Monosecret

The hosted release endpoints (`POST /api/release-commits` and `POST /api/release-requests`) read `MONOCHANGE_TOKEN` straight from the process environment. It was the only release secret that bypassed the app's secret manifest, so production never supplied it from 1Password.

`MONOCHANGE_TOKEN` is now an optional password entry in `app/monosecret.toml`. The production profile reads it from the `release` section of the `monochange.dev` item, and it is loaded into `AppState::api_token`; an empty value counts as unset. Behavior is unchanged: tokens are compared in constant time, an unconfigured token answers `503`, and a wrong token answers `401`.

Operators who set `MONOCHANGE_TOKEN` as a plain environment variable on the server must add it as a `MONOCHANGE_TOKEN` password field in the `release` section of the `monochange.dev` item instead. Deployments that rely only on GitHub Actions OIDC need no change.
