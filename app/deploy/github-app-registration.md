# GitHub App registration for repository connection and hosted releases

This checklist prepares a maintainer to connect selected repositories to `monochange.dev` and give monochange its GitHub bot identity. The App keeps the dashboard synchronized, creates reviewed release commits and pull requests, and provides the repository-scoped foundation for future issue and pull-request workflows. It does not publish packages or authorize registry operations.

First check whether the `monochange` organization already owns an appropriate GitHub App. Reuse its registration when possible; a public app page returning 404 does not prove that no private registration exists.

## Review the registration

[Open the prefilled GitHub registration form](https://github.com/organizations/monochange/settings/apps/new?name=monochange&description=Connect%20repositories%20to%20monochange.dev%20so%20monochange%20can%20keep%20the%20repositories%20you%20choose%20in%20sync%2C%20create%20reviewed%20release%20commits%20and%20pull%20requests%2C%20and%20support%20future%20issue%20workflows.&url=https%3A%2F%2Fmonochange.dev&setup_url=https%3A%2F%2Fmonochange.dev%2Fdashboard&setup_on_update=true&request_oauth_on_install=false&public=true&webhook_active=true&webhook_url=https%3A%2F%2Fmonochange.dev%2Fapi%2Fgithub%2Fwebhooks&metadata=read&contents=write&issues=write&pull_requests=write&events%5B%5D=issue_comment&events%5B%5D=issues&events%5B%5D=pull_request&events%5B%5D=pull_request_review&events%5B%5D=pull_request_review_comment&events%5B%5D=pull_request_review_thread) under the `monochange` organization. Opening the form does not submit it. The maintainer must review ownership, the available app name, visibility, and permissions before creating the app.

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
| Repository permissions                         | Metadata read-only; Contents, Issues, and Pull requests read/write                               |
| Organization, account, enterprise permissions  | No access                                                                                        |
| Event subscriptions                            | Issues, Issue comment, Pull request, Pull request review, review comment, and review thread      |
| Visibility                                     | Public, so installation can span personal accounts and organizations that choose to grant access |

GitHub delivers `installation` and `installation_repositories` events to every GitHub App automatically. Do not attempt to manually subscribe to those events. The explicit subscriptions prepare the App to receive issue and pull-request activity. The current webhook receiver processes installation lifecycle events and safely ignores other event names until their dedicated handlers ship.

Contents write permits the Git Database API operations used to create signed release commits and update release branches. Pull requests write permits creating and updating release pull requests. Issues write permits labels, issue creation, and comments shared by issue and pull-request workflows. Do not add Actions, Administration, Workflows, organization, account, or enterprise access without an implemented and reviewed API call that requires it. In particular, monochange must not modify `.github/workflows` through the hosted release API.

The existing organization OAuth App remains responsible for website sign-in at `https://monochange.dev/auth/callback`. Keep **Request user authorization during installation** disabled and leave the GitHub App callback URI blank. Enabling it would replace the post-installation setup redirect and introduce a second OAuth client that the website does not implement.

The form cannot prefill the webhook secret. See [GitHub's registration parameters](https://docs.github.com/en/apps/sharing-github-apps/registering-a-github-app-using-url-parameters) for the settings it can preselect.

## Configure the existing deployment

1. Save the actual App ID and generate a private key in the app's GitHub settings. The complete PEM file is required.
2. Store `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` together in the existing `monochange_app` production secret profile through the approved Monosecret workflow. Keep production access read-only and scoped; do not replace the development service account.
3. Deploy the reviewed website release through the normal automated release path. Do not install repositories while the old deployment silently discards installation webhooks.
4. Sign in with the existing website OAuth flow. The dashboard must offer a connection link resolved from the configured app's authenticated `GET /app` response. An invalid configuration must show an error; an unconfigured deployment must say connection is unavailable.
5. Install the app on a test account or organization. Choose all repositories or selected repositories on GitHub. The setup URL returns to the dashboard; query parameters such as `installation_id` never authorize access.
6. Verify a signed installation delivery succeeds in GitHub's delivery log, then refresh the dashboard. Verify public and private repository names, account isolation, repository additions/removals, suspension, and uninstall. Website sessions, app credentials, and installation tokens must never appear in browser output or public proof.

The installation owner comes from GitHub's signed webhook identity. Personal installations belong to their account owner; organization installations belong to the user who performed the installation while GitHub confirms they remain an organization owner. Verify that losing organization ownership prevents further dashboard access. Team-wide dashboard sharing is not part of this rollout.

## Capability boundaries

The registration grants the repository permissions needed by the existing hosted release commit and pull-request APIs. Those operations are only complete after production App credentials, GitHub Actions OIDC configuration, branch-protection behavior, and the live signed-commit flow are verified. Repository connection alone does not prove hosted release automation works.

Issue and pull-request event subscriptions reserve the event surface for mapping future user requests into GitHub collaboration. Do not process those events or advertise that workflow until its authorization model, deduplication, failure handling, tests, and live verification ship. Package publication remains a separate maintainer-controlled workflow.
