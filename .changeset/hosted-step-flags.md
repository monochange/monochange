---
monochange: fix
monochange_core: fix
---

# Honor hosted backend flags on release step commands

`--hosted-url` and `--oidc-audience` were accepted by `monochange step commit-release` but ignored, so a self-hosted app or a custom OIDC audience could only be set in `monochange.toml`. `--hosted-auth` on `monochange step open-release-request` rejected every value, including `oidc`, because the input declared no choices.

Both steps now resolve hosted settings in this order: step input (command-line flag or workflow `inputs`), then the `MONOCHANGE_HOSTED_URL` environment variable for the URL, then the step configuration, then the defaults (`https://monochange.dev` and an audience derived from its host). Empty values are ignored, so unset workflow inputs no longer shadow configuration, and an unknown `--hosted-auth` value fails instead of silently falling back.

```bash
monochange step open-release-request --backend hosted --hosted-auth oidc \
  --hosted-url https://release.example.com --oidc-audience release.example.com
```

Previously `monochange.toml` values took precedence over `MONOCHANGE_HOSTED_URL`; the environment variable now overrides the configured `hosted_url`, as its documentation described.
