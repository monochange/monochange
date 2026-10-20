---
monochange_app: feat
---

# Report installations that haven't approved the app's new permissions

GitHub keeps each GitHub App installation on the permissions it approved until an owner of the account accepts the app's newer request. The new `permission_updates` server function compares the app registration's permissions (`GET /app`) with each of the signed-in user's installations (`GET /app/installations/{id}`) and returns the accounts that are behind, the missing permissions, and the GitHub settings page where an owner reviews the request. The dashboard shows them in a notice above the repository connection panel.

`monochange_app_api::github_app` gains `GitHubAppAuth::requested_permissions`, `GitHubAppAuth::installation_permissions`, and `pending_permissions`, which lists permissions the app requests above the installation's level:

```rust
let requested = app.requested_permissions(&http).await?;
let granted = app.installation_permissions(&http, installation_id).await?;
let missing = monochange_app_api::github_app::pending_permissions(&requested, &granted);
```

A failed GitHub lookup leaves that account out of the list instead of failing the dashboard. No configuration changes are needed.
