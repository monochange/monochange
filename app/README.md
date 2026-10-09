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

The monochange GitHub App connects the repositories you grant it access to. Hosted release commits and pull requests under its bot identity require a separate rollout. The CLI remains available without the app, and monochange is free.

## Availability

Sign in at [monochange.dev](https://monochange.dev/login) and open the dashboard to check repository connection availability. When the deployment has a configured GitHub App, **Connect repositories on GitHub** opens that app's installation page. If connection is unavailable, the dashboard says so; the CLI and local release planning remain available.

Website sign-in identifies your account. Installing the GitHub App grants access to the repositories you choose. A connected repository does not mean hosted release automation is enabled: hosted bot operations still require a separate verified rollout.

## Install the GitHub App

When the dashboard offers repository connection:

1. Sign in and choose **Connect repositories on GitHub** in the dashboard.
2. Select the GitHub account or organization that owns the repository. An organization owner may need to approve the installation.
3. Choose **All repositories** for that account, or **Only select repositories** for a smaller selection. Repeat installation for another account or organization when needed.
4. Review the requested access. Repository connection needs the required metadata read permission. Contents and pull-request write access are only needed for a later hosted release workflow.
5. Complete installation and return to the dashboard. Refresh the repository list after GitHub delivers the installation webhook.

Personal installations belong to the account owner. Organization installations appear for the user who installed the app while GitHub confirms they remain an organization owner; the dashboard does not automatically share them with every organization member.

You can change the selected repositories, suspend access, or uninstall the app in GitHub's installed-app settings. Installing the bot does not publish packages automatically.

## Connect the release workflow

After hosted bot operations have been enabled and verified, configure the release workflow's commit and pull-request steps to use the hosted backend:

```toml
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", backend = "hosted" },
]
```

GitHub Actions authenticates with its OIDC token. Grant `id-token: write` to the job that calls the hosted release steps. Other CI systems can use a monochange API token stored as `MONOCHANGE_TOKEN` in the CI secret store; never commit it to the repository.

Every run rebuilds the release pull request branch from `[source.pull_requests].base` and the bot replaces the previous release commit, so each push refreshes the open release pull request. A run whose base branch moved while it was preparing the release fails with a conflict and leaves the refresh to the run for the newer commit.

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

## Website versions and release notes

<!-- {=websiteReleaseNotes} -->

The website at [monochange.dev](https://monochange.dev/changelog) uses monochange to version itself and display user-facing release notes. Its version is independent of the CLI. A website release never publishes a Cargo package.

## Register a website release target

The application keeps its version in `app/crates/monochange_app/Cargo.toml`. Register that private Cargo package with a namespaced release identity:

```toml
[package.monochange_app]
path = "app/crates/monochange_app"
additional_paths = ["app/**", "Dockerfile", ".github/workflows/ci.yml"]
publish = { enabled = false }
changelog = false
tag = true
release = true
version_format = "namespaced"
release_title = "monochange.dev {{ version }}"
changelog_version_title = "{{ version }}"
ignore_ecosystem_versioned_files = true
```

`changelog = false` disables the implicit developer-facing file for this target. The named outputs below retain developer notes separately from the public website history. `publish.enabled = false` keeps the application out of registry publication while retaining versioning and release records. `ignore_ecosystem_versioned_files = true` avoids stamping the repository's root Cargo manifest with the website version. The Cargo adapter still updates the app's native manifest and lockfile.

## Write for website users

Types choose the audience stream. Declare a website stream and types for features, fixes, and changes requiring user action:

```toml
[changelog.streams.website]
description = "Product updates for monochange.dev users. Excludes deployment, CI, credentials, and internal maintenance intended only for developers or operators."

[changelog.types]
website_feature = { bump = "minor", section = "website_added", stream = "website" }
website_fix = { bump = "patch", section = "website_fixed", stream = "website" }
website_change = { bump = "major", section = "website_changed", stream = "website" }

[changelog.sections]
website_added = { heading = "New", priority = 20 }
website_fixed = { heading = "Improved", priority = 30 }
website_changed = { heading = "Changed", priority = 10 }
```

Each changeset describes an outcome that visitors can recognize:

```markdown
---
monochange_app: website_feature
---

# Follow what's new on monochange.dev

Read website updates on the What's new page, with versions separate from the CLI.
```

The same `monochange_app` package uses default-stream types such as `fix` for deployment credentials, CI, and internal maintenance intended only for operators. Those changes need no website entry. When a change also affects visitors, write a separate website changeset describing that visible outcome. Bumps follow the repository's SemVer policy, including its policy for versions below 1.0.

## Generate Markdown and JSON from the same notes

Keep a developer changelog alongside the cumulative website Markdown output and one immutable public JSON file per website version:

```toml
[changelog.outputs.website_developer]
stream = "default"
targets = ["monochange_app"]
path = "app/developer-changelog.md"
format = "keep_a_changelog"
mode = "append"
initial_header = "# Developer updates for monochange.dev\n\nRelease notes for website developers and operators.\n"

[changelog.outputs.website]
stream = "website"
targets = ["monochange_app"]
path = "app/changelog.md"
format = "keep_a_changelog"
mode = "append"
initial_header = "# What's new on monochange.dev\n\nUser-facing website release notes.\n"

[changelog.outputs.website_json]
stream = "website"
targets = ["monochange_app"]
path = "app/public/releases/{{ version }}.json"
format = "json"
mode = "release"
```

Change section headings, priorities, or Markdown entry templates in `monochange.toml`. JSON keeps structured entry fields such as `summary`, `details_markdown`, `stream`, and provenance. Markdown templates do not turn those fields into Markdown strings.

Preview the named output before preparing a release:

```bash
monochange step validate
monochange preview --format json
monochange notes --output website_json --target monochange_app
monochange notes --output website_developer --target monochange_app
```

Inspect `output`, `stream`, `owner_id`, and `path` in the dry-run artifacts. The notes command renders prospective notes without changing package versions or consuming changesets. Release preparation commits the JSON files with the version bump and the durable release record.

## Render the notes in your product

The website's `get_website_releases` server function reads the JSON artifacts shipped in its application image. It validates version filenames and stream identity, sorts versions with SemVer, and converts Markdown details to sanitized HTML. Leptos renders summaries as escaped text. A missing history has an explicit first-release state; malformed files produce an error instead of silently removing updates.

The `/changelog` components and stylesheet own the visual layout. Change those files to customize the presentation without changing the release-note generator. The public JSON files at `/releases/<version>.json` can also feed another website, an in-app update view, or an update feed.

## Deploy after release approval

The existing release workflow prepares a release PR. A maintainer approves and merges that PR through the repository's required checks. Post-merge automation creates release tags and draft releases, then runs the `website-deployment` job in `ci.yml` only when the release record contains `monochange_app`.

The deployment job reads its dedicated SSH key directly from the `website-production` environment and validates it before setting up tooling or building an image. It verifies the exact commit, tag, and recorded website artifacts, then builds and smoke-tests a Linux image, transfers it through a pinned SSH connection, backs up SQLite, and restarts the application with its existing runtime credentials. It removes the temporary key even if an earlier step fails. Public HTTPS, health, JSON, and changelog checks must pass before automation publishes the website's GitHub release with the recorded user notes. CLI publication remains separate.

Production deployments are serialized. Failed deployments leave the website release as a draft and report the failure in Actions. Inspect migrations and backups before a rollback, because an older image may not support the new database schema. Re-run the failed deployment job after fixing its cause; do not create another tag or release record.

The workflow needs a dedicated `WEBSITE_DEPLOY_SSH_KEY` secret in the `website-production` Actions environment and the corresponding forced-command key on the server. Restrict that environment to `main`. Keep the key separate from personal SSH access. Production passwords stay in the runtime vault, and the private deployment key belongs in the recovery vault. See `app/deploy/digitalocean/OPERATIONS.md` for the server setup and credential rotation procedure.

<!-- {/websiteReleaseNotes} -->
