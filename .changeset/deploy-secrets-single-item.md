---
monochange_app: refactor
---

# Keep deployment credentials in the `monochange.dev` 1Password item

`app/deploy/digitalocean/monosecret.toml` now reads `DIGITALOCEAN_ACCESS_TOKEN` from the `deploy` section of the `monochange.dev` item in the `monochange` vault, through the operator's own signed-in 1Password app. It no longer reads the local system keyring, and the separate `monochange deployment` recovery vault is retired. The same `deploy` section holds recovery copies of `WEBSITE_DEPLOY_SSH_KEY` and the production `OP_SERVICE_ACCOUNT_TOKEN`, so every monochange credential lives in one vault and one item.

Operators who kept the token in the keyring must add it to 1Password once:

| Section  | Field                       | Type     |
| -------- | --------------------------- | -------- |
| `deploy` | `DIGITALOCEAN_ACCESS_TOKEN` | password |

Commands are unchanged:

```bash
ms run --file app/deploy/digitalocean/monosecret.toml --reason "inspect production Droplet" -- devenv shell doctl compute droplet get 606211709
```

The app never reads the `deploy` section, but the production service account can, so keep the DigitalOcean token narrowly scoped and expiring. `app/DEPLOY.md` also now says the old SecretSpec items keep their value in a `password` field, not `value`.
