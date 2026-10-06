# monochange_app

Leptos SSR web app for monochange release planning.

## local development

The app uses SQLite by default, so no database service is required.

```bash
# From the repository root
devenv shell cargo leptos --manifest-path app/crates/monochange_app/Cargo.toml serve
```

Default database:

```text
sqlite://.devenv/state/monochange_app.sqlite3
```

Override with `DATABASE_URL` when needed:

```bash
DATABASE_URL=sqlite://./monochange_app.sqlite3 devenv shell cargo leptos --manifest-path app/crates/monochange_app/Cargo.toml serve
```

## tests

```bash
devenv shell cargo test --manifest-path app/Cargo.toml -p monochange_app_db --lib
devenv shell cargo check --manifest-path app/Cargo.toml -p monochange_app
```

Release automation uses the same SQLite database and remains disabled unless `MONOCHANGE_APP_AUTOMATION` is explicitly enabled.

## deploy

See [DEPLOY.md](./DEPLOY.md) for the DigitalOcean Docker deployment guide.

## monochange GitHub App

<!-- {=hostedAppInstallation} -->

The monochange GitHub App adds a bot identity to the hosted release workflow. It creates release commits and release pull requests for the repositories you grant it access to. The CLI remains available without the app, and monochange is free.

## Availability

Check [the installation page](https://monochange.dev/install#github-app) for current hosted app availability. The public GitHub App installation is still being finalized; the CLI and local release planning are available now. GitHub sign-in on the website is separate from installing the bot on a repository.

## Install the GitHub App

Once installation is available:

1. Open the installation page and choose **Install on GitHub**.
2. Select the GitHub account or organization that owns the repository. An organization owner may need to approve the installation.
3. Choose **Only select repositories** and select the repositories monochange should manage.
4. Review the requested access. The hosted release workflow needs repository contents and pull-request write access; GitHub also grants the required metadata read access.
5. Complete installation, then follow the hosted workflow configuration below.

You can change the selected repositories, suspend access, or uninstall the app in GitHub's installed-app settings. Installing the bot does not publish packages automatically.

## Connect the release workflow

Configure the release workflow's commit and pull-request steps to use the hosted backend:

```toml
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", backend = "hosted" },
]
```

GitHub Actions authenticates with its OIDC token. Grant `id-token: write` to the job that calls the hosted release steps. Other CI systems can use a monochange API token stored as `MONOCHANGE_TOKEN` in the CI secret store; never commit it to the repository.

Keep your repository's branch protection and required checks enabled. Review and merge the bot's release pull request through your normal process; registry publishing remains a separate workflow.

<!-- {/hostedAppInstallation} -->

## Hosted backend configuration

Endpoints (all under `monochange.dev` in production):

- `POST /api/github/webhooks` — installation lifecycle events; verifies the `X-Hub-Signature-256` HMAC with the app webhook secret and keeps the `installations` and `repositories` tables in sync.
- `POST /api/release-commits` — hosted `CommitRelease` backend. Authenticates with a GitHub Actions OIDC token (audience `monochange.dev`) or the `MONOCHANGE_TOKEN` API token, mints a one-hour installation token, and creates blobs/tree/commit through the Git Database API with a branch-moved guard.
- `POST /api/release-requests` — hosted `OpenReleaseRequest` backend. Opens or updates the release pull request under the bot identity, applies configured labels, and enables auto-merge when requested.

Required runtime secrets (beyond the website's OAuth secrets):

- `GITHUB_APP_ID` — the monochange GitHub App id.
- `GITHUB_APP_PRIVATE_KEY` — the app private key (PEM).
- `GITHUB_APP_WEBHOOK_SECRET` — the app webhook secret.
- `MONOCHANGE_OIDC_AUDIENCE` — the audience required in OIDC tokens (defaults to `monochange.dev`).
- `MONOCHANGE_TOKEN` — the API token offered to repositories as the non-OIDC fallback secret (optional; omit to require OIDC only).

When the app credentials are absent the website still runs; the hosted endpoints respond with `503` until the deployment configures them.
