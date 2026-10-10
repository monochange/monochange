---
monochange_app: refactor
---

# Load monochange.dev secrets through Monosecret from one 1Password item

Production startup now reads every credential from a single 1Password item with one batched read, instead of one SecretSpec item per secret. This keeps the app well under the 1Password service-account request quota. Before deploying this release, operators must copy the production values into the new item and install the new Compose file.

`app/secretspec.toml` is replaced by `app/monosecret.toml`. The container selects its profile with `MONOSECRET_PROFILE` and no longer sets a provider:

```yaml
# before
environment:
  SECRETSPEC_PROFILE: production
  SECRETSPEC_PROVIDER: onepassword://monochange

# after
environment:
  MONOSECRET_PROFILE: production
```

The production profile reads the `monochange.dev` item in the `monochange` vault. Each secret is a field labelled with its name, in a section named after its group:

| Section   | Fields                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------ |
| `auth`    | `JWT_SECRET`                                                                                                       |
| `github`  | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, `GITHUB_APP_WEBHOOK_SECRET` |
| `release` | `MONOCHANGE_OIDC_AUDIENCE` (optional)                                                                              |
| `ai`      | `OPENROUTER_API_KEY` (optional)                                                                                    |

`DATABASE_URL` now comes from the Compose environment, and the bootstrap `OP_SERVICE_ACCOUNT_TOKEN` still comes from the Docker secret. The image keeps the bundled `op` CLI because Monosecret's 1Password provider shells out to it.

To migrate production:

1. Create the `monochange.dev` item with the sections and fields above, copying each value from its `secretspec/monochange_app/production/<KEY>` item.
2. Install `app/deploy/digitalocean/docker-compose.yml` from this release on the server before the image changes. The new image would treat a leftover `SECRETSPEC_PROVIDER` as an override and fail to start.
3. Deploy, then check `/health`, the app logs, sign-in, and the repository connection link.
4. Delete the old `secretspec/monochange_app/production/<KEY>` items.

Local development uses the `development` profile, which reads the ignored `app/.env`, then the environment, then local defaults, and never contacts 1Password. See "production cutover from SecretSpec to Monosecret" in `app/DEPLOY.md` for the full procedure and rollback.
