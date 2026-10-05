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

The app hosts the bot that creates release commits and release pull requests for repositories that install the monochange GitHub App.

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
