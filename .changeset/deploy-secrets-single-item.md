---
monochange_app: refactor
---

# Keep deployment credentials in the `monochange` 1Password vault

`app/deploy/digitalocean/monosecret.toml` now reads `DIGITALOCEAN_ACCESS_TOKEN` from the `deploy` item in the `monochange` vault, through the operator's own signed-in 1Password app. It no longer reads the local system keyring, and the separate `monochange deployment` recovery vault is retired. The same `deploy` item holds recovery copies of `WEBSITE_DEPLOY_SSH_KEY` and the read-only production `OP_SERVICE_ACCOUNT_TOKEN`, next to the app's `monochange.dev` runtime item, so every monochange credential lives in one vault.

Operators who kept the token in the keyring must add it to 1Password once:

| Item     | Field                       | Type     |
| -------- | --------------------------- | -------- |
| `deploy` | `DIGITALOCEAN_ACCESS_TOKEN` | password |

Commands are unchanged:

```bash
ms run --file app/deploy/digitalocean/monosecret.toml --reason "inspect production Droplet" -- devenv shell doctl compute droplet get 606211709
```

The app never reads the `deploy` item, but the production service account can, so keep the DigitalOcean token narrowly scoped and expiring. `app/DEPLOY.md` also now says the old SecretSpec items keep their value in a `password` field, not `value`.
