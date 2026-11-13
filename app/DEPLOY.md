# deploy monochange.dev

This guide covers how `monochange_app`, the Rust/Leptos SSR website and hosted API behind `https://monochange.dev`, reaches production, what an operator checks before each website release, how to recover a failed rollout, and how to rebuild the server from scratch.

Production is one hardened DigitalOcean Droplet running Docker Compose with two containers: the `monochange_app` image built from this repository and Caddy for TLS. SQLite lives on the Droplet disk, and every runtime credential is read from one 1Password item through Monosecret.

- [Release-day checklist](#release-day-checklist): what to do before merging a release PR that bumps `monochange_app`.
- [How a release reaches production](#how-a-release-reaches-production): the automated path, step by step.
- [Recover a failed deployment](#recover-a-failed-deployment): diagnose, fix, re-run, or roll back.
- [Runtime configuration](#runtime-configuration): secrets in 1Password and optional settings in `app.env`.
- [Build a new production server](#build-a-new-production-server): the one-time bootstrap.
- [CLI operations](deploy/digitalocean/OPERATIONS.md): SSH access, manual deploys, the DigitalOcean API token, OS updates, the CVE-2026-98432 mitigation, and DNS.

## production at a glance

| Item                 | Value                                                                                                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------- |
| Site                 | `https://monochange.dev`; `app.monochange.dev` and `www.monochange.dev` redirect to it                        |
| Droplet              | `606211709` (`monochange-app`), `lon1`, `s-1vcpu-1gb`, IPv4 `167.172.51.148`                                  |
| SSH                  | `ssh -F app/deploy/digitalocean/ssh_config monochange-production` (user `deploy`, pinned host key)            |
| Deploy directory     | `/opt/monochange` (Compose file, Caddyfile, `.env`, `app.env`, `data/`, `backups/`, `secrets/`)               |
| Database             | `/opt/monochange/data/monochange_app.sqlite3`, mounted in the container as `/data/monochange_app.sqlite3`     |
| Runtime secrets      | 1Password item `monochange.dev` in vault `monochange` (Kickjump account), read by a read-only service account |
| Operator credentials | 1Password item `deploy` in the same vault                                                                     |
| Automated deployment | `.github/workflows/app.yml`, job `website-deployment`, Actions environment `website-production`               |
| Deployed version     | `curl -fsS https://monochange.dev/health \| jq -r .version`                                                   |
| Release tag          | `monochange_app/vX.Y.Z`; its GitHub release stays a draft until the deployment is verified                    |

## release-day checklist

`monochange_app` releases ride along with the normal release PR (`chore(release): prepare release`). The release PR body lists `package monochange_app -> monochange_app/vX.Y.Z` when the website is part of the batch. Merging it deploys the website automatically, so do these checks **before** a maintainer merges it.

1. **Find what is deployed now.**

   ```bash
   curl -fsS https://monochange.dev/health | jq
   gh release list --repo monochange/monochange --limit 10
   ```

   `version` is the running image. A `monochange_app/vX.Y.Z` release still marked _Draft_ means that version never finished deploying; read [recover a failed deployment](#recover-a-failed-deployment) first.

2. **Diff the deployment inputs between the deployed tag and the release PR.**

   ```bash
   git fetch origin --tags monochange/release/release
   git diff "monochange_app/v<deployed-version>" origin/monochange/release/release -- \
     app/monosecret.toml Dockerfile app/deploy
   ```

   The automated deployment only ships the image. It never copies `docker-compose.yml`, `Caddyfile`, or the helper scripts to the server, and it never edits 1Password. Anything that changed in that diff needs the matching manual step below.

3. **Add new required secrets.** A field added to `[profiles.production]` in `app/monosecret.toml` with `required = true` must exist in the `monochange.dev` item before the new image starts, or the app exits at startup and the deployment fails its health check. See [production secret layout](#production-secret-layout) and the [release notes for operators](#release-notes-for-operators).

4. **Install changed server files first.** Copying a Compose file or Caddyfile does not restart anything, so it is safe to do before the merge:

   ```bash
   scp -F app/deploy/digitalocean/ssh_config \
     app/deploy/digitalocean/docker-compose.yml app/deploy/digitalocean/Caddyfile \
     monochange-production:/opt/monochange/
   ```

   For changed helper scripts, see [update the server helpers](#update-the-server-helpers). Reload Caddy only when the Caddyfile changed: `ssh -F app/deploy/digitalocean/ssh_config monochange-production 'cd /opt/monochange && docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile'`.

5. **Check GitHub App permissions** when the release notes mention a new GitHub capability. The [GitHub App registration checklist](deploy/github-app-registration.md) lists the permissions the current code uses. GitHub asks each installation owner to approve added permissions, and the dashboard shows a review link until they do.

6. **Merge the release PR** (maintainer only) and watch the `app` workflow run for the release commit. It needs about 15 minutes for the image build, then deploys. Expect the website's GitHub release to switch from draft to published when it succeeds.

### release notes for operators

| Release                | Operator action before merging                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `monochange_app` 0.4.0 | Add the required `TOKEN_ENCRYPTION_KEY` field to the `auth` section of `monochange.dev` (any long random string, for example from `openssl rand -base64 48`). Tokens stored before 0.4.0 stay readable and are sealed on their next refresh. Changing the key later signs every user out of GitHub. Copy the 0.4.0 `docker-compose.yml`, which disables Multipath TCP for Caddy.                                                                                                                                                                                                                                                                                                                                                                |
| `monochange_app` 0.5.0 | Grant the GitHub App **Actions: Read and write** and approve it on the installation that covers `monochange/monochange` and `monochange/actions`, so `publish.yml` can dispatch the actions release through `POST /api/workflow-dispatches`. That last `publish.yml` step calls the new endpoint, so it fails if the website deployment has not finished or failed; packages and the CLI release are already public by then, so once the website is healthy just run the `release` workflow in `monochange/actions` by hand. The workspace moves to `/projects` with redirects from old bookmarks. Set `MONOCHANGE_EXTENSION_REDIRECT_URIS` in `app.env` before the browser extension can sign in. Additive database migrations run at startup. |
| `monochange_app` 0.5.0 | Server files: copy the new `docker-compose.yml` and reinstall `monochange-deploy` (see [update the server helpers](#update-the-server-helpers)). The helper now waits up to 120 seconds for health and prints the app's startup logs when it fails. Optional settings move to `/opt/monochange/app.env`.                                                                                                                                                                                                                                                                                                                                                                                                                                        |

Add a row here whenever a website change needs an operator step, and say which release it shipped in.

## how a release reaches production

```text
merge release PR ──► push to main
  ├─ ci.yml  release-post-merge: create tags + draft releases, dispatch publish.yml (CLI and packages)
  └─ app.yml select-website-release ──► website-deployment (environment: website-production)
        wait for tag ─► verify release identity ─► docker build ─► verify-image.sh
        ─► docker save | ssh (forced command) ─► ci-deploy.sh ─► monochange-deploy ─► /health
        ─► public checks ─► publish the monochange_app GitHub release
```

1. **Trigger.** `app.yml` runs on every push to `main` that changes `.monochange/releases/**`. `select-website-release` reads the release record of the pushed commit and stops unless it is a release commit with a `monochange_app` target. Library-only releases leave the website alone.
2. **Wait for the tag.** `release-post-merge` in `ci.yml` creates the `monochange_app/vX.Y.Z` tag and draft release concurrently. The deployment polls for the tag for up to 30 minutes.
3. **Verify identity.** The job checks that `HEAD`, the tag, and the release record all name the same commit on `main`, and that `app/public/releases/X.Y.Z.json` and the website release notes exist.
4. **Build and smoke-test.** It builds the repository `Dockerfile` on the runner and runs `app/deploy/verify-image.sh`, which starts the image with test credentials and checks `/health`, privilege dropping, the main pages, the WASM assets, precompression, and the release JSON. A failure here leaves production untouched.
5. **Transfer.** It streams `docker save | gzip` over SSH with the `WEBSITE_DEPLOY_SSH_KEY` secret. That key is restricted on the server to the forced command `/usr/local/bin/monochange-ci-deploy` (`ci-deploy.sh`), which takes a lock, refuses to replace a newer website version, loads the image, backs up SQLite, and runs `monochange-deploy`.
6. **Roll out.** `monochange-deploy` (`monochange-deploy.sh`) writes `MONOCHANGE_IMAGE=monochange-app:<sha>` to `/opt/monochange/.env`, runs `docker compose up -d --pull never`, and waits up to 120 seconds for `http://127.0.0.1:3000/health`. On success `ci-deploy.sh` records the version in `/opt/monochange/.website-version`.
7. **Verify publicly and publish notes.** The job checks `https://monochange.dev/health` reports the new version, `/changelog` lists it, and `/releases/X.Y.Z.json` serves. Only then does it publish the `monochange_app/vX.Y.Z` GitHub release with the website notes.

Deployments are serialized by the `monochange-website-production` concurrency group and the server lock. The workflow builds the exact tagged commit, so re-running it rebuilds the same image.

## recover a failed deployment

A failed `website-deployment` leaves the website release as a draft. Where it failed decides what production is running:

| Failed step                                             | Production state                                                                                           |
| ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Any step up to and including `verify the release image` | Unchanged, still the previous version                                                                      |
| `transfer and deploy through the restricted key`        | Compose already points at the **new** image; if it never became healthy, the site is down or crash-looping |
| `verify public website and publish its release notes`   | New version running locally; check Caddy, DNS, and the public checks                                       |

1. **Read the startup logs.** The deploy helper prints them in the Actions log when the health check fails. To read them directly:

   ```bash
   ssh -F app/deploy/digitalocean/ssh_config monochange-production \
     'cd /opt/monochange && cat .env .website-version; docker compose ps; docker compose logs --tail=100 app'
   ```

   A secret error names the missing field, for example `TOKEN_ENCRYPTION_KEY`. A database error names the failed migration. An invalid `MONOCHANGE_RATE_LIMIT_*` or `MONOCHANGE_APP_AUTOMATION*` value also stops startup.

2. **Fix the cause.** Add the missing 1Password field, correct `app.env`, or install the changed Compose file. Then restart the app and confirm it is healthy:

   ```bash
   ssh -F app/deploy/digitalocean/ssh_config monochange-production \
     'cd /opt/monochange && docker compose up -d --force-recreate app && sleep 20 && curl -fsS http://127.0.0.1:3000/health'
   ```

3. **Finish the release.** Open the failed `app` run in Actions and choose **Re-run failed jobs**. Re-running is safe: it rebuilds the same commit, `ci-deploy.sh` accepts the same version again, and the job publishes the draft release after the public checks pass. Do not dispatch `app.yml` from `main` instead: it only deploys when the current `main` commit is itself the release commit, so it does nothing once later commits have landed.

4. **Roll back only when a fix is not quick.** Images from earlier deployments stay loaded on the Droplet. Pick the previous one and deploy it with your personal key, which is not restricted to the forced command:

   ```bash
   ssh -F app/deploy/digitalocean/ssh_config monochange-production \
     'docker image ls monochange-app --format "{{.Tag}} {{.CreatedSince}}"'
   ssh -F app/deploy/digitalocean/ssh_config monochange-production \
     '/usr/local/bin/monochange-sqlite-backup && /usr/local/bin/monochange-deploy monochange-app:<previous-full-sha>'
   ```

   Image rollback and database restore are different tools, and nothing downgrades the database automatically. Follow this checklist, in order:

   1. **Back up first, always before any schema change runs**: `/usr/local/bin/monochange-sqlite-backup` takes an online `sqlite3 .backup` snapshot while the app is running. Note the timestamp of the snapshot you made.
   2. **Prefer the image rollback.** The candidate's migrations are additive: new tables (`oauth_state_uses`, `client_grants`, `client_sessions`) and one new column (`users.session_epoch`, default 0), with every 0.3.3 column, row, and value preserved — proven by the release-upgrade rehearsal (`monochange_app_db` tests and `scripts/sqlite-backup-rehearsal.ts`). An older image's reads keep working against the upgraded schema, and its migration runner no-ops on migrations it does not know. This is reader/runner compatibility, not proof the old binary was re-run; watch its startup logs after it starts.
   3. **Restore only if an old image cannot run against the newer schema**, and treat a restore as discarding every write newer than the snapshot: stop the app, verify the snapshot before replacing anything (`sqlite3 /opt/monochange/backups/<snapshot>.sqlite3 'PRAGMA quick_check; PRAGMA integrity_check;'` — refuse a snapshot that fails either check), export anything you need from the newer data first, then copy the snapshot over `/opt/monochange/data/monochange_app.sqlite3` as the `deploy` user and start the app again. No helper restores automatically.
   4. **Preserve the surrounding state**: keep the Droplet's SQLite engine and bind mounts (`/opt/monochange/data`, `/opt/monochange/caddy`, `/opt/monochange/backups`, root-owned `/opt/monochange/secrets` with the server-owned service-account token) exactly as they are, keep Caddy running — its TLS state lives in `/opt/monochange/caddy` — and keep the previous image plus the deployment receipts (`.env`, `.website-version`, the draft GitHub release) until the recovered deployment is verified healthy.
   5. **Know the limit**: backups are Droplet-local (14-day retention via `find -mtime +14`); no off-Droplet copy exists yet. If the Droplet itself is lost, recover through [build a new production server](#build-a-new-production-server) and restore the latest snapshot onto it — anything newer than that snapshot, and anything never backed up, is gone.

   A newer website release supersedes a failed one, but it needs the same fix before it can start.

## runtime configuration

### production secret layout

The `production` profile in `app/monosecret.toml` reads every credential from the item titled `monochange.dev` in the `monochange` vault. Each value is a field whose label is the secret name, inside a section named after its group. Monosecret matches section and field labels case-insensitively. A startup costs one 1Password auth probe and one batched item read, however many fields the item carries.

| Section   | Field                       | Field type | Required | Value                                                                                                                                                                                                                      |
| --------- | --------------------------- | ---------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `auth`    | `JWT_SECRET`                | password   | yes      | Session JWT signing secret                                                                                                                                                                                                 |
| `auth`    | `TOKEN_ENCRYPTION_KEY`      | password   | yes      | Key that encrypts GitHub OAuth tokens in the database (required since `monochange_app` 0.4.0); any non-empty string works, and changing it invalidates stored tokens (users sign in again)                                 |
| `github`  | `GITHUB_CLIENT_ID`          | text       | yes      | GitHub App client ID                                                                                                                                                                                                       |
| `github`  | `GITHUB_CLIENT_SECRET`      | password   | yes      | GitHub App client secret                                                                                                                                                                                                   |
| `github`  | `GITHUB_APP_ID`             | text       | no       | GitHub App ID                                                                                                                                                                                                              |
| `github`  | `GITHUB_APP_PRIVATE_KEY`    | password   | no       | The complete PEM private key, including the `BEGIN` and `END` lines                                                                                                                                                        |
| `github`  | `GITHUB_APP_WEBHOOK_SECRET` | password   | no       | GitHub App webhook secret                                                                                                                                                                                                  |
| `release` | `MONOCHANGE_OIDC_AUDIENCE`  | text       | no       | Audience required in GitHub Actions OIDC tokens; the app uses `monochange.dev` when it is empty                                                                                                                            |
| `release` | `MONOCHANGE_TOKEN`          | password   | no       | API token the hosted release endpoints accept from CI systems without GitHub Actions OIDC; leave it empty to require OIDC. It only acts on repositories listed in the `MONOCHANGE_TOKEN_REPOSITORIES` setting in `app.env` |
| `ai`      | `OPENROUTER_API_KEY`        | password   | no       | OpenRouter API key                                                                                                                                                                                                         |

`GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` enable repository connection and must be configured together. The app validates the PEM at startup and refuses to start with a partial set or a malformed key.

Operator credentials live beside it in a Secure Note titled `deploy` in the same `monochange` vault, as top-level password fields. The app never reads this item.

| Field                       | Field type | Value                                                                                                   |
| --------------------------- | ---------- | ------------------------------------------------------------------------------------------------------- |
| `DIGITALOCEAN_ACCESS_TOKEN` | password   | Scoped DigitalOcean API token that operators resolve through `app/deploy/digitalocean/monosecret.toml`  |
| `WEBSITE_DEPLOY_SSH_KEY`    | password   | Recovery copy of the forced-command deployment key held in the `website-production` Actions environment |
| `OP_SERVICE_ACCOUNT_TOKEN`  | password   | Recovery copy of the read-only production service account token stored on the server                    |

Because the production service account reads the whole vault, deployed code can read the `deploy` item too; keep the DigitalOcean token scoped and short-lived as described in [CLI operations](deploy/digitalocean/OPERATIONS.md#api-credential).

Two production values stay out of the item. Compose sets `DATABASE_URL=sqlite:///data/monochange_app.sqlite3` next to the volume it names, and the entrypoint exports `OP_SERVICE_ACCOUNT_TOKEN` from the Docker secret.

### optional settings in app.env

Settings that are not secrets live in `/opt/monochange/app.env`, which Compose loads into the app container when the file exists. `monochange-deploy` rewrites `.env` with the image tag on every rollout, so never put settings there. Values in the Compose file's `environment` block win over `app.env`.

```bash
ssh -t -F app/deploy/digitalocean/ssh_config monochange-production \
  'touch /opt/monochange/app.env && chmod 0640 /opt/monochange/app.env && vi /opt/monochange/app.env'
ssh -F app/deploy/digitalocean/ssh_config monochange-production 'cd /opt/monochange && docker compose up -d --force-recreate app'
```

Use one `NAME=value` per line, for example `MONOCHANGE_TOKEN_REPOSITORIES=monochange/monochange,monochange/actions`.

`--force-recreate` makes Compose start a fresh container that reads the new settings; a plain `restart` keeps the old environment.

| Variable                              | Default                                         | Purpose                                                                                                                                                                                                                                                                           |
| ------------------------------------- | ----------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MONOCHANGE_TOKEN_REPOSITORIES`       | empty                                           | `owner/repo` entries, separated by commas or spaces, that the `MONOCHANGE_TOKEN` secret may act on. Empty leaves that token unable to act anywhere.                                                                                                                               |
| `MONOCHANGE_EXTENSION_REDIRECT_URIS`  | empty                                           | Exact `https` redirect URIs from `identity.getRedirectURL()` for each published browser extension build. Empty means no extension can sign in.                                                                                                                                    |
| `MONOCHANGE_NATIVE_REDIRECT_URIS`     | `dev.monochange.app:/auth/native/return`        | Redirect URIs the native app's sign-in handoff may return to.                                                                                                                                                                                                                     |
| `MONOCHANGE_OIDC_AUDIENCE`            | `release` secret, then `monochange.dev`         | Audience required in GitHub Actions OIDC tokens. The environment value wins over the secret.                                                                                                                                                                                      |
| `MONOCHANGE_GITHUB_CALLBACK_URL`      | `https://monochange.dev/auth/callback`          | GitHub App user-authorization callback. It also defines the site origin for same-origin checks, so change it only with the domain.                                                                                                                                                |
| `MONOCHANGE_FEEDBACK_MODEL`           | empty                                           | OpenRouter model for feedback conversations. Empty, or no `OPENROUTER_API_KEY`, keeps deterministic replies without model calls.                                                                                                                                                  |
| `MONOCHANGE_FEEDBACK_COMPLETIONS_URL` | `https://openrouter.ai/api/v1/chat/completions` | Chat-completions endpoint for that model.                                                                                                                                                                                                                                         |
| `MONOCHANGE_RATE_LIMIT_*`             | see [API rate limits](#api-rate-limits)         | Per-client limits; a value that is not a whole number above zero stops startup.                                                                                                                                                                                                   |
| `MONOCHANGE_APP_AUTOMATION`           | off                                             | Background release-automation worker, tuned by `MONOCHANGE_APP_AUTOMATION_MODE` (`dry-run`), `MONOCHANGE_APP_AUTOMATION_TICK_SECONDS` (`30`), and `MONOCHANGE_APP_AUTOMATION_WORKER_ID`. Only dry-run mode exists, so leave it off in production. An invalid value stops startup. |
| `RUST_LOG`                            | `info` (image default)                          | Log filter, for example `info,monochange_app=debug` while debugging.                                                                                                                                                                                                              |

Compose sets `MONOSECRET_PROFILE=production`, `DATABASE_URL`, and `OP_SERVICE_ACCOUNT_TOKEN_FILE` itself; the image sets `LEPTOS_*` and `PORT`. Do not override them in `app.env`.

### how secrets reach the app

1. Compose mounts `/opt/monochange/secrets/op_service_account_token` (root-owned, mode `0600`) as `/run/secrets/onepassword_service_account_token`.
2. The entrypoint (`app/deploy/docker-entrypoint.sh`) reads it as root, exports `OP_SERVICE_ACCOUNT_TOKEN`, and uses `setpriv` to drop to UID/GID 1000 with no capabilities and `no_new_privs` before starting the server.
3. `monochange_app` loads `monosecret.toml` with `MONOSECRET_PROFILE=production`. Monosecret runs the bundled `op` CLI once to read the `monochange.dev` item: one auth probe and one batched item read per startup.
4. A missing required field stops startup with an error naming it. Optional GitHub App fields must be all present or all absent, and the PEM must parse.

### GitHub App

The same GitHub App signs users in and connects repositories. Follow the [GitHub App registration checklist](deploy/github-app-registration.md) for its settings: homepage `https://monochange.dev`, callback `https://monochange.dev/auth/callback`, webhook `https://monochange.dev/api/github/webhooks` with SSL verification, user authorization during installation, and expiring user tokens. Do not register a second OAuth app.

If the logs warn that the GitHub App credentials are not configured, the dashboard shows "Repository connection is unavailable". Add `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` to the `github` section of the `monochange.dev` item, then run `docker compose restart app`; secrets are read at every start.

### API rate limits

The app limits Leptos server functions, REST endpoints under `/api`, and the `/login` and `/auth/callback` pages before server rendering. OAuth pages always use the address bucket, including for signed-in visitors. Other pages, `/pkg` assets, `/health`, and `/api/github/webhooks` are never limited: GitHub doesn't redeliver a dropped webhook on its own, and each delivery is already authenticated by its signature.

| Client                    | Counted by                       | Sustained      | Burst |
| ------------------------- | -------------------------------- | -------------- | ----- |
| Signed in (valid session) | user id                          | 300 per minute | 100   |
| Anyone else, including CI | client address (IPv6 by its /64) | 60 per minute  | 30    |

Each limit is a GCRA bucket from the [`governor`](https://docs.rs/governor) crate: a client may spend the burst at once, then it refills at the sustained rate. Limited responses carry the IETF [`RateLimit-Policy` and `RateLimit`](https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/) fields, and a `429 Too Many Requests` adds `Retry-After` in seconds. Server functions reject with an encoded `ServerFnError`, so the site shows the message; REST endpoints reject with their usual `{"message": ...}` JSON body.

OAuth page rejections return plain text. Callback state is consumed once in SQLite before the provider exchange, so a copied state cookie cannot trigger another exchange, including after a restart. Failed exchanges require a fresh sign-in attempt. The startup migration creates `oauth_state_uses`; expired entries are pruned as callbacks arrive. Token exchanges time out after ten seconds, and each app process allows at most sixteen concurrent exchanges. Capacity rejections preserve the pending state for a retry.

Override any limit in [`app.env`](#optional-settings-in-appenv); each must be a whole number above zero, and the app refuses to start otherwise.

| Variable                                         | Default |
| ------------------------------------------------ | ------- |
| `MONOCHANGE_RATE_LIMIT_ANONYMOUS_PER_MINUTE`     | `60`    |
| `MONOCHANGE_RATE_LIMIT_ANONYMOUS_BURST`          | `30`    |
| `MONOCHANGE_RATE_LIMIT_AUTHENTICATED_PER_MINUTE` | `300`   |
| `MONOCHANGE_RATE_LIMIT_AUTHENTICATED_BURST`      | `100`   |

The client address comes from `X-Forwarded-For` only when the connecting peer is a loopback or private address, which in this deployment is Caddy on the Docker network. Caddy [drops the `X-Forwarded-For` a client sends](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy#defaults) and writes the real address, and the app reads the header from the right, so a client can't pick its own bucket. If another proxy or load balancer ever sits in front of Caddy, configure Caddy's `trusted_proxies` for it, or every client will share that proxy's address. Limits live in memory, so a restart resets them and each app instance counts on its own.

## update the server helpers

`harden-droplet.sh` installs three helpers on a new server. On the running server, replace a helper when its source changes, without rerunning the hardening:

| Source in `app/deploy/digitalocean/` | Installed as                          | Used by                                                                                  |
| ------------------------------------ | ------------------------------------- | ---------------------------------------------------------------------------------------- |
| `monochange-deploy.sh`               | `/usr/local/bin/monochange-deploy`    | `ci-deploy.sh` and manual deployments                                                    |
| `ci-deploy.sh`                       | `/usr/local/bin/monochange-ci-deploy` | the forced command of the Actions key                                                    |
| `mitigate-cve-2026-98432.sh`         | systemd unit and nftables rules       | see [OPERATIONS.md](deploy/digitalocean/OPERATIONS.md#cve-2026-98432-interim-mitigation) |

```bash
scp app/deploy/digitalocean/monochange-deploy.sh app/deploy/digitalocean/ci-deploy.sh root@167.172.51.148:/root/
ssh root@167.172.51.148 'install -o root -g root -m 0755 /root/monochange-deploy.sh /usr/local/bin/monochange-deploy \
  && install -o root -g root -m 0755 /root/ci-deploy.sh /usr/local/bin/monochange-ci-deploy \
  && rm /root/monochange-deploy.sh /root/ci-deploy.sh'
```

Root login is key-only. The helpers are root-owned so the `deploy` user, and therefore the Actions key, cannot change them.

## backups

`harden-droplet.sh` installs `/usr/local/bin/monochange-sqlite-backup` and a daily cron entry. Each run writes a consistent `sqlite3 .backup` snapshot to `/opt/monochange/backups/monochange_app-<UTC timestamp>.sqlite3` and deletes snapshots older than 14 days. Every deployment takes one more snapshot before the restart.

The mechanism is rehearsed against the real release schema: `scripts/sqlite-backup-rehearsal.ts` (its `vitest` suite lives in `scripts/npm/tests/`) builds the upgrade-rehearsal database from `fixtures/tests/app-sqlite-release-upgrade/`, writes committed rows into an uncheckpointed WAL file, takes the same `sqlite3 .backup` snapshot, and restores it into a different directory. The rehearsal proves three things an operator can rely on: a snapshot taken while the app is writing contains every committed row (a torn copy of just the database file misses the rows still in the WAL); a restored snapshot passes `quick_check`, `integrity_check`, and `foreign_key_check`; and restoring verifies the snapshot before replacing anything, so a corrupt or missing snapshot fails and leaves the last-good database untouched.

Off-Droplet copies are not set up yet. The preferred first option is `rclone` to Backblaze B2 or DigitalOcean Spaces; use `restic` later if encrypted, deduplicated backups become important. To restore, stop the app, verify the snapshot first (`sqlite3 <snapshot>.sqlite3 'PRAGMA quick_check; PRAGMA integrity_check;'`), export anything you need from the newer data, copy the verified snapshot over `/opt/monochange/data/monochange_app.sqlite3` as the `deploy` user, and start the app again. A restore is always deliberate: no workflow, helper, or cron job replaces the live database on its own.

## build a new production server

Do this only for a replacement Droplet. Routine releases never need it. Initial provisioning needs a DigitalOcean credential that can create Droplets; the routine `monochange deployment` token deliberately cannot.

### target shape

- Droplet: Basic 1 GiB RAM minimum, `s-1vcpu-1gb`, Ubuntu 24.04, region `lon1`.
- Persistent data: `/opt/monochange/data` on the Droplet disk.
- TLS: Caddy with automatic Let's Encrypt certificates.
- Secrets: one 1Password item read by a read-only service account; only that account's token is copied to the server.
- Backups: SQLite `.backup` snapshots plus an off-Droplet copy.

A DigitalOcean Block Storage volume is not part of the design. It attaches to one Droplet at a time, so it does not make SQLite active/active, does not shorten deployments, and does not let two Droplets share one database. See [path to lower downtime](#path-to-lower-downtime).

### 1. create the Droplet

```bash
REGION=lon1
DROPLET_NAME=monochange-app
SSH_KEY_ID="<chosen-key-id>"

doctl compute droplet create "$DROPLET_NAME" \
  --region "$REGION" \
  --image ubuntu-24-04-x64 \
  --size s-1vcpu-1gb \
  --ssh-keys "$SSH_KEY_ID" \
  --wait

DROPLET_ID="<id-from-create-output>"
DROPLET_IP="$(doctl compute droplet get "$DROPLET_ID" --format PublicIPv4 --no-header)"
echo "$DROPLET_IP"
```

Verify the new host key through the authenticated DigitalOcean console, then update `HostName` in `app/deploy/digitalocean/ssh_config`, the key in `app/deploy/digitalocean/known_hosts`, and the [production at a glance](#production-at-a-glance) table. Never disable host-key checking.

Point the root and `app` A records at `DROPLET_IP`, and `www` at `monochange.dev`, before starting Caddy. DNS is at Namecheap; see [DNS through the API](deploy/digitalocean/OPERATIONS.md#dns-through-the-api).

### 2. create the cloud firewall

```bash
# Optional, if your SSH source address is stable:
# export SSH_SOURCES="$(curl -fsS https://ifconfig.me)/32"
DROPLET_ID=$DROPLET_ID app/deploy/digitalocean/create-firewall.sh
```

It allows inbound `22/tcp` from `SSH_SOURCES` (default: anywhere), inbound `80` and `443` from anywhere, and all outbound traffic for packages, image pulls, 1Password, GitHub, and ACME. Pass `FIREWALL_ID` to attach the existing `monochange-app` firewall instead of creating one.

### 3. harden the server

Copy the hardening script with its sibling helpers; it refuses to run without them.

```bash
scp app/deploy/digitalocean/harden-droplet.sh \
  app/deploy/digitalocean/monochange-deploy.sh \
  app/deploy/digitalocean/ci-deploy.sh \
  app/deploy/digitalocean/mitigate-cve-2026-98432.sh \
  root@$DROPLET_IP:/root/
ssh root@$DROPLET_IP 'chmod +x /root/*.sh && /root/harden-droplet.sh'
```

The script:

- installs Docker and the Compose plugin from Docker's apt repository, plus `sqlite3`, `fail2ban`, `nftables`, `ufw`, and `unattended-upgrades`;
- creates the `deploy` user in the `docker` group with root's authorized keys;
- sets SSH to keys only (`PasswordAuthentication no`, `KbdInteractiveAuthentication no`, `PermitRootLogin prohibit-password`, `AllowUsers deploy root`);
- enables UFW (deny incoming except `22`, `80`, `443`) and a fail2ban SSH jail;
- enables unattended security upgrades with an automatic reboot at `07:30` UTC when one is needed;
- creates `/opt/monochange/{data,backups,caddy}` owned by `deploy` and `/opt/monochange/secrets` owned by root with mode `0700`;
- installs `monochange-deploy`, `monochange-ci-deploy`, and the daily SQLite backup;
- installs the [CVE-2026-98432 interim mitigation](deploy/digitalocean/OPERATIONS.md#cve-2026-98432-interim-mitigation).

Use `deploy@$DROPLET_IP` for deployments from now on and `root@` only for privileged maintenance. Membership of the `docker` group makes `deploy` effectively root, so treat its keys as production access.

### 4. install the Compose files and the service-account token

```bash
scp app/deploy/digitalocean/docker-compose.yml app/deploy/digitalocean/Caddyfile deploy@$DROPLET_IP:/opt/monochange/
ssh root@$DROPLET_IP 'install -m 600 /dev/stdin /opt/monochange/secrets/op_service_account_token' <<'EOF'
ops_...
EOF
```

Take the token from the `OP_SERVICE_ACCOUNT_TOKEN` field of the `deploy` item. Create `/opt/monochange/app.env` now if production needs any [optional settings](#optional-settings-in-appenv).

### 5. add the Actions deployment key

Generate a dedicated key; never reuse a personal key in CI.

```bash
ssh-keygen -t ed25519 -N '' -C monochange-website-actions -f ./monochange-website-actions
printf 'restrict,command="/usr/local/bin/monochange-ci-deploy" %s\n' "$(cat ./monochange-website-actions.pub)" |
  ssh deploy@$DROPLET_IP 'cat >>~/.ssh/authorized_keys'
```

Store the private key as the `WEBSITE_DEPLOY_SSH_KEY` secret of the `website-production` Actions environment (restricted to `main`) and as the `WEBSITE_DEPLOY_SSH_KEY` field of the `deploy` item, then delete both local files. The key can only run `deploy <full-sha> <version>` with an image on stdin: no shell, PTY, or forwarding. Rotation and the other CLI details are in [OPERATIONS.md](deploy/digitalocean/OPERATIONS.md#automated-website-releases).

### 6. first deployment

Re-run the `website-deployment` job of the `app` workflow run for the latest website release, or deploy a commit by hand as described in [SSH checks and updates](deploy/digitalocean/OPERATIONS.md#ssh-checks-and-updates). Both end with `monochange-deploy monochange-app:<full-sha>`, which starts Caddy and the app. Restore the latest SQLite backup into `/opt/monochange/data` first when you are replacing a server rather than starting fresh.

### 7. verify

```bash
curl -fsS https://monochange.dev/health | jq
ssh -F app/deploy/digitalocean/ssh_config monochange-production 'cd /opt/monochange && docker compose ps && docker compose logs --tail=100 app'
```

Expected health response, with the deployed version:

```json
{
	"status": "ok",
	"http": "up",
	"version": "0.5.0"
}
```

Then sign in on `https://monochange.dev`, open `/projects`, and confirm the dashboard offers the repository connection link.

## path to lower downtime

Each deployment restarts the single app container, so SSR and API requests can fail for a few seconds. Static assets may still be served from caches. Safe next steps, in order:

1. Add a container readiness check and health-gated restart.
2. Run two app containers on the same Droplet behind Caddy for blue/green, with one SQLite writer at a time.
3. Keep only one writer active during migration windows.
4. For multiple Droplets or zero-downtime writes, move from SQLite to PostgreSQL, Turso/libSQL, or another networked database.

Never mount the same SQLite database as a writable database on more than one Droplet.

## appendix: completed migrations

### production cutover from SecretSpec to Monosecret

This one-time migration was completed when `monochange_app` 0.3.0 was deployed on 2026-10-19. It stays here for reference and for rolling back to a pre-0.3.0 image. Releases before the Monosecret migration read one 1Password item per secret, titled `secretspec/monochange_app/production/<KEY>`, each holding its value in a concealed `password` field. The current image reads the single `monochange.dev` item instead. Moving the values is a one-time operator task. Do it by hand in the 1Password app, never through the production service account, which stays read-only.

1. **Create the item.** In the `monochange` vault, create a Secure Note titled exactly `monochange.dev`. Add the sections `auth`, `github`, `release`, and `ai`.
2. **Add the fields.** For each row in the [production secret layout](#production-secret-layout), add a field labelled with the secret name to its section, with the listed field type. Copy the value from the `password` field of the matching `secretspec/monochange_app/production/<KEY>` item. Skip optional fields that have no old item. Do not copy `secretspec/monochange_app/production/DATABASE_URL`; Compose now supplies it. Paste `GITHUB_APP_PRIVATE_KEY` as the complete PEM, including the `BEGIN` and `END` lines.
3. **Check the item.** The item must hold exactly these fields: `JWT_SECRET` and `TOKEN_ENCRYPTION_KEY` in `auth`; `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` in `github`; optional `MONOCHANGE_OIDC_AUDIENCE` and `MONOCHANGE_TOKEN` in `release`; and optional `OPENROUTER_API_KEY` in `ai`. The production service account keeps read access to the `monochange` vault, and the token file on the server does not change.
4. **Install the new Compose file before the image changes.** Monosecret still honours the old `SECRETSPEC_PROVIDER` variable as a provider override, so the new image started with the old Compose file would bypass the item and fail to start. Copy `app/deploy/digitalocean/docker-compose.yml` from the release revision to `/opt/monochange/` as described in [CLI operations](deploy/digitalocean/OPERATIONS.md). Replacing the file does not restart the running container.
5. **Deploy.** Let the automated website deployment ship the release, or run the manual update from [CLI operations](deploy/digitalocean/OPERATIONS.md). The deploy helper waits for `/health`.
6. **Verify.** Confirm `curl -fsS https://monochange.dev/health | jq` reports `"status": "ok"`. Check `docker compose logs --tail=100 app` for a clean start with no secret or GitHub App errors. Sign in on `https://monochange.dev`, open the dashboard, and confirm the repository connection link is offered. A missing required field stops the app at startup with an error naming the secret.
7. **Delete the old items.** Only after verification, delete every `secretspec/monochange_app/production/<KEY>` item from the `monochange` vault, including `DATABASE_URL`.

To roll back before step 7, restore the previous Compose file and redeploy the previous image; the old items are still in place. After step 7, rolling back to a SecretSpec image requires recreating those items.
