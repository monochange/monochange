# deploy monochange_app on DigitalOcean

These are the production deployment steps for `monochange_app`. The initial target is one hardened DigitalOcean Droplet running Docker Compose, Caddy, the Rust/Leptos SSR app, and SQLite on the Droplet disk.

## target shape

- Droplet: Basic 1 GiB RAM minimum, e.g. `s-1vcpu-1gb`.
- Region: choose the closest practical region, e.g. `lon1`.
- Persistent data: host directory at `/opt/monochange/data`.
- Database: SQLite file at `/opt/monochange/data/monochange_app.sqlite3` on the host, mounted into the app as `/data/monochange_app.sqlite3`.
- TLS/reverse proxy: Caddy container with automatic Let's Encrypt certificates.
- Secrets: app loads `monosecret.toml` through the Monosecret Rust SDK and reads every production credential from one 1Password item; the container only receives the 1Password service account token as a Docker secret.
- Backups: SQLite `.backup` snapshots plus off-Droplet copy to object storage.

## why no DigitalOcean block volume initially

DigitalOcean Block Storage Volumes can only be attached to one Droplet at a time for this use case. They are useful when you want to detach data from one Droplet and attach it to a replacement Droplet in the same region, or resize/snapshot that disk independently.

They do **not** make SQLite active/active, do **not** reduce deployment downtime by themselves, and do **not** allow two app Droplets to safely write the same SQLite database. Since our initial plan is one Droplet with offsite backups, the extra volume lifecycle is not worth the complexity yet.

If we later need lower downtime or multiple app instances, the right move is not a shared block volume. The right move is either:

- same-Droplet blue/green app containers with one SQLite writer; or
- moving the database to PostgreSQL, Turso/libSQL, or another networked database before multi-Droplet active/active.

## automation and hardening approach

Use `doctl` for infrastructure and SSH for app deployment. The repository's development shell includes both tools. Follow [CLI operations](deploy/digitalocean/OPERATIONS.md) for the existing production server, scoped API authentication, verified SSH, repeatable image updates, and the DNS API migration.

Recommended hardening baseline:

- disable password SSH login;
- use SSH keys only;
- create a non-root deploy user;
- install Docker from the official apt repository;
- enable UFW and allow only `22`, `80`, and `443`;
- install and enable `fail2ban`;
- enable unattended security upgrades;
- keep `/opt/monochange/secrets/op_service_account_token` mode `0600`;
- keep the 1Password service account scoped to the minimal production vault/items;
- let the entrypoint read the root-only token, then drop privileges to the non-root `app` user before starting the server;
- use off-Droplet backups.

## prerequisites

- `doctl` authenticated through Monosecret; use a scoped API token rather than storing a second plaintext copy with `doctl auth init`:

```bash
devenv shell doctl version
```

- `monochange.dev`, `app.monochange.dev`, and `www.monochange.dev` pointed at the Droplet IPv4 address; Caddy serves `monochange.dev` and redirects the aliases to it so host-only sign-in cookies use the same host as the OAuth callback;
- the monochange GitHub App configured for user authorization with callback URL:

```text
https://monochange.dev/auth/callback
```

- a 1Password service account scoped to the monochange production secrets;
- the production values stored as fields of one 1Password item, laid out as described in [production secret layout](#production-secret-layout).

Create a dedicated `monochange` vault and give the production service account read access to that vault only. Do not grant write access or reuse the shared development service account. Keep the service account token separately as a password item for recovery; it is the only credential copied to the server.

### production secret layout

The `production` profile in `app/monosecret.toml` reads every credential from the item titled `monochange.dev` in the `monochange` vault. Each value is a field whose label is the secret name, inside a section named after its group. Monosecret matches section and field labels case-insensitively. A startup costs one 1Password auth probe and one batched item read, however many fields the item carries.

| Section   | Field                       | Field type | Required | Value                                                                                                                     |
| --------- | --------------------------- | ---------- | -------- | ------------------------------------------------------------------------------------------------------------------------- |
| `auth`    | `JWT_SECRET`                | password   | yes      | Session JWT signing secret                                                                                                |
| `github`  | `GITHUB_CLIENT_ID`          | text       | yes      | GitHub App client ID                                                                                                      |
| `github`  | `GITHUB_CLIENT_SECRET`      | password   | yes      | GitHub App client secret                                                                                                  |
| `github`  | `GITHUB_APP_ID`             | text       | no       | GitHub App ID                                                                                                             |
| `github`  | `GITHUB_APP_PRIVATE_KEY`    | password   | no       | The complete PEM private key, including the `BEGIN` and `END` lines                                                       |
| `github`  | `GITHUB_APP_WEBHOOK_SECRET` | password   | no       | GitHub App webhook secret                                                                                                 |
| `release` | `MONOCHANGE_OIDC_AUDIENCE`  | text       | no       | Audience required in GitHub Actions OIDC tokens; the app uses `monochange.dev` when it is empty                           |
| `release` | `MONOCHANGE_TOKEN`          | password   | no       | API token the hosted release endpoints accept from CI systems without GitHub Actions OIDC; leave it empty to require OIDC |
| `ai`      | `OPENROUTER_API_KEY`        | password   | no       | OpenRouter API key                                                                                                        |

`GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` enable repository connection and must be configured together. The app validates the PEM at startup and refuses to start with a partial set or a malformed key.

Two production values stay out of the item. Compose sets `DATABASE_URL=sqlite:///data/monochange_app.sqlite3` next to the volume it names, and the entrypoint exports `OP_SERVICE_ACCOUNT_TOKEN` from the Docker secret.

For repository connection and hosted release work, follow the [GitHub App registration checklist](deploy/github-app-registration.md). Use homepage `https://monochange.dev`, user authorization callback `https://monochange.dev/auth/callback`, and an active webhook at `https://monochange.dev/api/github/webhooks` with SSL verification enabled. Enable user authorization during installation and keep expiring user tokens enabled. Grant repository Metadata read access and Contents, Issues, and Pull requests read/write access, plus Organization Members read access. Subscribe to issue, issue-comment, pull-request, review, review-comment, review-thread, push, and repository events; GitHub delivers installation lifecycle events automatically. Keep account, enterprise, Actions, Administration, and Workflows access disabled. Keep the client secret and webhook secret in the production vault and the complete PEM private key out of source control.

The same GitHub App client ID and client secret authorize website users and installation onboarding. Do not register a second OAuth app or change DNS to enable repository connection.

## 1. create the Droplet

The routine deployment token is intentionally unable to create Droplets or list SSH keys. Initial provisioning requires a separate, appropriately scoped maintainer credential. Reuse the existing production target for app updates.

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

Point the root and `app` A records at `DROPLET_IP`, and point `www` at `monochange.dev`, before starting Caddy.

## 2. create the DigitalOcean firewall

Create a cloud firewall before starting the app. By default this allows SSH, HTTP, and HTTPS. If your SSH source IP is stable, set `SSH_SOURCES` to that CIDR before running the script.

```bash
# Optional, more secure if your IP is stable:
# export SSH_SOURCES="$(curl -fsS https://ifconfig.me)/32"

DROPLET_ID=$DROPLET_ID \
  app/deploy/digitalocean/create-firewall.sh
```

Cloud firewall rules:

- inbound `22/tcp` from `SSH_SOURCES` (defaults to the internet for portability);
- inbound `80/tcp` and `443/tcp` from the internet;
- outbound TCP/UDP to the internet for package installs, registry pulls, 1Password, GitHub, and ACME.

## 3. bootstrap and harden the server

Run the hardened setup script on the Droplet. It installs Docker, configures SSH key-only access, creates the `deploy` user, enables UFW/fail2ban/unattended upgrades, creates app directories, and installs backup/deploy helper scripts.

```bash
scp app/deploy/digitalocean/harden-droplet.sh root@$DROPLET_IP:/root/harden-droplet.sh
ssh root@$DROPLET_IP 'chmod +x /root/harden-droplet.sh && /root/harden-droplet.sh'
```

The script enforces:

- `PasswordAuthentication no`;
- `KbdInteractiveAuthentication no`;
- `ChallengeResponseAuthentication no`;
- `PubkeyAuthentication yes`;
- `PermitRootLogin prohibit-password`;
- `AllowUsers deploy root` so `deploy` is used for normal deploys while root remains key-only for privileged maintenance;
- UFW default-deny incoming, allow `22`, `80`, `443`;
- fail2ban SSH jail;
- unattended security upgrades;
- `/opt/monochange/secrets` mode `0700`;
- `/opt/monochange/secrets/op_service_account_token` should be written with mode `0600`.

After this step, use `deploy@$DROPLET_IP` for ordinary deployment and `root@$DROPLET_IP` only for privileged maintenance.

## 4. copy deploy files and bootstrap the 1Password token

```bash
scp app/deploy/digitalocean/docker-compose.yml deploy@$DROPLET_IP:/tmp/docker-compose.yml
scp app/deploy/digitalocean/Caddyfile deploy@$DROPLET_IP:/tmp/Caddyfile
ssh root@$DROPLET_IP 'install -o deploy -g deploy -m 0644 /tmp/docker-compose.yml /opt/monochange/docker-compose.yml && install -o deploy -g deploy -m 0644 /tmp/Caddyfile /opt/monochange/Caddyfile && rm /tmp/docker-compose.yml /tmp/Caddyfile'
```

Store the 1Password service account token as a Docker secret source file on the host:

```bash
ssh root@$DROPLET_IP 'install -m 600 /dev/stdin /opt/monochange/secrets/op_service_account_token' <<'EOF'
ops_...
EOF
```

Runtime flow:

1. Compose mounts `/opt/monochange/secrets/op_service_account_token` as `/run/secrets/onepassword_service_account_token`.
2. The entrypoint reads the root-owned `0600` file, exports it as `OP_SERVICE_ACCOUNT_TOKEN`, and uses `setpriv` to switch to UID/GID 1000 with no capabilities or new privileges. Local Compose file-backed secrets retain host file ownership, so the read must happen before dropping privileges.
3. `monochange_app` loads `monosecret.toml` through the Monosecret SDK with `MONOSECRET_PROFILE=production`.
4. Monosecret resolves `OP_SERVICE_ACCOUNT_TOKEN` from the environment, then runs the bundled `op` CLI once to read the `monochange.dev` item. Monosecret's 1Password provider always shells out to `op`, so the image still ships the CLI.
5. The typed secret set is stored in `AppState` for server handlers.

## 5. build and upload the Docker image

Manual first deploy from the repo root:

```bash
docker build -t monochange-app:latest .
docker save monochange-app:latest | gzip | ssh root@$DROPLET_IP 'gunzip | docker load'
```

Use a commit-specific image tag for updates. The CLI operations guide uploads an image archive over SSH, so deployment does not require publishing to a registry. A future registry workflow requires a separate maintainer decision.

## 6. start the app

```bash
ssh root@$DROPLET_IP 'cd /opt/monochange && docker compose up -d'
```

## 7. verify

```bash
curl -fsS https://monochange.dev/health | jq
ssh root@$DROPLET_IP 'cd /opt/monochange && docker compose ps && docker compose logs --tail=100 app'
```

Expected health response:

```json
{
	"status": "ok",
	"http": "up"
}
```

## 8. SQLite backups

Droplet backups are useful, but they are not enough. Keep SQLite-consistent backups and copy them off-Droplet.

Install a local backup script:

```bash
ssh root@$DROPLET_IP 'cat >/usr/local/bin/monochange-sqlite-backup <<EOF
#!/usr/bin/env bash
set -euo pipefail
DB=/opt/monochange/data/monochange_app.sqlite3
BACKUP_DIR=/opt/monochange/backups
STAMP=\$(date -u +%Y%m%dT%H%M%SZ)
mkdir -p "\$BACKUP_DIR"
sqlite3 "\$DB" ".backup \$BACKUP_DIR/monochange_app-\$STAMP.sqlite3"
find "\$BACKUP_DIR" -name "monochange_app-*.sqlite3" -mtime +14 -delete
EOF
chmod +x /usr/local/bin/monochange-sqlite-backup
cat >/etc/cron.daily/monochange-sqlite-backup <<EOF
#!/usr/bin/env bash
set -euo pipefail
/usr/local/bin/monochange-sqlite-backup
EOF
chmod +x /etc/cron.daily/monochange-sqlite-backup'
```

Add offsite sync next. Preferred first option: `rclone` to Backblaze B2 or DigitalOcean Spaces. Later, use `restic` if encrypted deduplicated backups become important.

## GitHub Actions deploy access

Use separate SSH keys for local access and CI deploys. Do not put a personal laptop private key in GitHub Actions.

Recommended setup:

- local key: your normal SSH key or a dedicated `monochange_do` key;
- CI key: a separate `github-actions-monochange-deploy` ed25519 key;
- both public keys are added to `/home/deploy/.ssh/authorized_keys`;
- GitHub Actions stores only the CI private key in a production environment secret.

Create the CI key locally:

```bash
ssh-keygen -t ed25519 -C "github-actions-monochange-deploy" -f ./monochange_actions_deploy
cat ./monochange_actions_deploy.pub | ssh root@$DROPLET_IP 'cat >>/home/deploy/.ssh/authorized_keys'
rm ./monochange_actions_deploy.pub
```

Store `./monochange_actions_deploy` as a GitHub Actions environment secret named `DO_SSH_PRIVATE_KEY`, then delete the local private key copy after storing it. Also store:

- `DO_HOST`: Droplet IP or hostname;
- `DO_USER`: `deploy`.

For stronger CI lockdown, use a forced-command wrapper that validates the requested commit image and invokes `/usr/local/bin/monochange-deploy` with that image. The deploy helper requires `monochange-app:<full-commit-sha>` as an argument; an argument-free forced command will fail. The wrapper and CI grant must be implemented and reviewed before adding that key. Local deploy keys remain available for maintenance.

DigitalOcean's API can manage infrastructure, firewalls, images, and Droplets, but it is not a general remote-command API for a raw Droplet. For `docker compose up -d`, SSH or a small deploy agent is still required. SSH with a dedicated deploy key and optional forced command is the simplest secure path.

## update deploy

Follow [CLI operations](deploy/digitalocean/OPERATIONS.md) to build and upload a commit-specific image, back up SQLite, and invoke the health-checked deploy helper as `deploy`.

Expected downtime for this initial deploy shape is small, usually one app restart window. Static assets may remain cached by Cloudflare/Caddy, but SSR/API requests can fail during the restart.

## production cutover from SecretSpec to Monosecret

Releases before the Monosecret migration read one 1Password item per secret, titled `secretspec/monochange_app/production/<KEY>`, each holding its value in a concealed field named `value`. The current image reads the single `monochange.dev` item instead. Moving the values is a one-time operator task. Do it by hand in the 1Password app, never through the production service account, which stays read-only.

1. **Create the item.** In the `monochange` vault, create a Secure Note titled exactly `monochange.dev`. Add the sections `auth`, `github`, `release`, and `ai`.
2. **Add the fields.** For each row in the [production secret layout](#production-secret-layout), add a field labelled with the secret name to its section, with the listed field type. Copy the value from the `value` field of the matching `secretspec/monochange_app/production/<KEY>` item. Skip optional fields that have no old item. Do not copy `secretspec/monochange_app/production/DATABASE_URL`; Compose now supplies it. Paste `GITHUB_APP_PRIVATE_KEY` as the complete PEM, including the `BEGIN` and `END` lines.
3. **Check the item.** The item must hold exactly these fields: `JWT_SECRET` in `auth`; `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` in `github`; optional `MONOCHANGE_OIDC_AUDIENCE` and `MONOCHANGE_TOKEN` in `release`; and optional `OPENROUTER_API_KEY` in `ai`. The production service account keeps read access to the `monochange` vault, and the token file on the server does not change.
4. **Install the new Compose file before the image changes.** Monosecret still honours the old `SECRETSPEC_PROVIDER` variable as a provider override, so the new image started with the old Compose file would bypass the item and fail to start. Copy `app/deploy/digitalocean/docker-compose.yml` from the release revision to `/opt/monochange/` as described in [CLI operations](deploy/digitalocean/OPERATIONS.md). Replacing the file does not restart the running container.
5. **Deploy.** Let the automated website deployment ship the release, or run the manual update from [CLI operations](deploy/digitalocean/OPERATIONS.md). The deploy helper waits for `/health`.
6. **Verify.** Confirm `curl -fsS https://monochange.dev/health | jq` reports `"status": "ok"`. Check `docker compose logs --tail=100 app` for a clean start with no secret or GitHub App errors. Sign in on `https://monochange.dev`, open the dashboard, and confirm the repository connection link is offered. A missing required field stops the app at startup with an error naming the secret.
7. **Delete the old items.** Only after verification, delete every `secretspec/monochange_app/production/<KEY>` item from the `monochange` vault, including `DATABASE_URL`.

To roll back before step 7, restore the previous Compose file and redeploy the previous image; the old items are still in place. After step 7, rolling back to a SecretSpec image requires recreating those items.

## path to lower downtime

A separate block volume is not the path to lower downtime for SQLite. Safe next steps:

1. Add a container readiness check and health-gated restart.
2. Add Caddy with two local app upstreams for same-Droplet blue/green.
3. Keep only one writer active during migration windows.
4. If we need multi-Droplet or true zero-downtime writes, move from SQLite to PostgreSQL, Turso/libSQL, or another networked DB.

Do not mount the same SQLite database as an active writable database across multiple Droplets.
