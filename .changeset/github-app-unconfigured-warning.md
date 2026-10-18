---
monochange_app: fix
---

# Warn at startup when the GitHub App is not configured

The app now logs a warning at startup when `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` are all unset. Before this change, a deployment without these values started silently and the dashboard only reported "Repository connection is unavailable", so operators had no server-side signal pointing at the missing credentials.

Configuration is unchanged. To enable repository connection in production, add the three values as fields of the `github` section in the `monochange.dev` item of the `monochange` vault and restart the app:

```bash
docker compose restart app
docker compose logs --tail=100 app
```
