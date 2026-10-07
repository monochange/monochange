# GitHub App registration for repository connection

This checklist prepares a maintainer to enable repository connection at `monochange.dev`. It does not register an app, change production secrets, install repositories, or enable hosted release automation.

First check whether the `monochange` organization already owns an appropriate GitHub App. Reuse its registration when possible; a public app page returning 404 does not prove that no private registration exists.

## Review the registration

[Open the prefilled GitHub registration form](https://github.com/organizations/monochange/settings/apps/new?name=monochange&description=Repository%20connections%20for%20monochange&url=https%3A%2F%2Fmonochange.dev&setup_url=https%3A%2F%2Fmonochange.dev%2Fdashboard&setup_on_update=true&request_oauth_on_install=false&public=true&webhook_active=true&webhook_url=https%3A%2F%2Fmonochange.dev%2Fapi%2Fgithub%2Fwebhooks&metadata=read) under the `monochange` organization. Opening the form does not submit it. The maintainer must review ownership, the available app name, visibility, and permissions before creating the app.

| Setting                                        | Value for this rollout                                                                           |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Owner                                          | `monochange` organization, with ownership verified by the maintainer                             |
| App name                                       | `monochange` if available; the website resolves the actual slug from authenticated app metadata  |
| Homepage                                       | `https://monochange.dev`                                                                         |
| Setup URL                                      | `https://monochange.dev/dashboard`                                                               |
| Redirect on update                             | Enabled                                                                                          |
| Request user authorization during installation | Disabled; website OAuth sign-in is separate                                                      |
| GitHub App user authorization callback         | Leave blank for this rollout                                                                     |
| Webhook                                        | Active, `https://monochange.dev/api/github/webhooks`, SSL verification enabled                   |
| Webhook secret                                 | A new secret securely stored by the maintainer; never put it in a URL or source file             |
| Repository permissions                         | Metadata read only                                                                               |
| Other permissions                              | No access                                                                                        |
| Visibility                                     | Public, so installation can span personal accounts and organizations that choose to grant access |

GitHub delivers `installation` and `installation_repositories` events to every GitHub App automatically. Do not attempt to manually subscribe to those events. This rollout does not need push, pull-request, workflow, or organization-membership subscriptions. See [GitHub's webhook event reference](https://docs.github.com/en/webhooks/webhook-events-and-payloads#installation_repositories).

The form cannot prefill the webhook secret. See [GitHub's registration parameters](https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-using-url-parameters) for the settings it can preselect.

## Configure the existing deployment

1. Save the actual App ID and generate a private key in the app's GitHub settings. The complete PEM file is required.
2. Store `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` together in the existing `monochange_app` production secret profile through the approved Monosecret workflow. Keep production access read-only and scoped; do not replace the development service account.
3. Deploy the reviewed website release through the normal automated release path. Do not install repositories while the old deployment silently discards installation webhooks.
4. Sign in with the existing website OAuth flow. The dashboard must offer a connection link resolved from the configured app's authenticated `GET /app` response. An invalid configuration must show an error; an unconfigured deployment must say connection is unavailable.
5. Install the app on a test account or organization. Choose all repositories or selected repositories on GitHub. The setup URL returns to the dashboard; query parameters such as `installation_id` never authorize access.
6. Verify a signed installation delivery succeeds in GitHub's delivery log, then refresh the dashboard. Verify public and private repository names, account isolation, repository additions/removals, suspension, and uninstall. Website sessions, app credentials, and installation tokens must never appear in browser output or public proof.

The installation owner comes from GitHub's signed webhook identity. Personal installations belong to their account owner; organization installations belong to the user who performed the installation while GitHub confirms they remain an organization owner. Verify that losing organization ownership prevents further dashboard access. Team-wide dashboard sharing is not part of this rollout.

## Hosted release automation remains a separate rollout

Repository connection and listing use metadata access. The existing hosted release commit and pull-request APIs need Contents and Pull requests write permissions, further workflow configuration, and their own live verification. Do not grant those permissions or advertise working bot operations solely because repositories appear in the dashboard. Package publication remains a separate maintainer-controlled workflow.
