# DigitalOcean deployment assets

This directory holds the server-side files for the `monochange.dev` Droplet. `app/DEPLOY.md` explains when each one changes and how to install it.

| File                          | Installed as                                                                                   |
| ----------------------------- | ---------------------------------------------------------------------------------------------- |
| `docker-compose.yml`          | `/opt/monochange/docker-compose.yml`, copied by hand; automated releases never update it       |
| `Caddyfile`                   | `/opt/monochange/Caddyfile`, copied by hand                                                    |
| `harden-droplet.sh`           | Run once as root on a new Droplet, with the helper scripts beside it                           |
| `monochange-deploy.sh`        | `/usr/local/bin/monochange-deploy`, the health-checked rollout helper                          |
| `monochange-sqlite-backup.sh` | `/usr/local/bin/monochange-sqlite-backup`, the consistent SQLite snapshot helper               |
| `ci-deploy.sh`                | `/usr/local/bin/monochange-ci-deploy`, the forced command of the GitHub Actions deployment key |
| `mitigate-cve-2026-98432.sh`  | The interim nftables mitigation and its systemd unit                                           |
| `create-firewall.sh`          | Run locally with `doctl` to create or attach the cloud firewall                                |
| `monosecret.toml`             | Run locally to resolve the DigitalOcean API token from the `deploy` 1Password item             |
| `ssh_config`, `known_hosts`   | Run locally: the pinned SSH target `monochange-production`                                     |

The production app loads secrets with the Monosecret Rust SDK from one 1Password item, `monochange.dev` in the `monochange` vault. Docker Compose only injects the bootstrap 1Password service account token as a Docker secret from `/opt/monochange/secrets/op_service_account_token`, and loads optional non-secret settings from `/opt/monochange/app.env`.

Persistent SQLite data is stored on the Droplet at `/opt/monochange/data` and mounted into the app container as `/data`. Day-to-day operations are in `OPERATIONS.md`.

Manual calls require `monochange-deploy monochange-app:<40hex> <stable X.Y.Z>`. `--rollback` explicitly permits an older image without restoring a database. `--bootstrap` authorizes a first empty database only when no deployment history exists. CI accepts exactly `deploy <40hex> <stable X.Y.Z>` and cannot request either mode.

Reinstall all three helpers together and install jq, sqlite3, util-linux, curl, gzip and coreutils. Update daily cron to `monochange-sqlite-backup --daily`. Move legacy `.env` settings to `app.env` manually without evaluating shell text; retain JWT/encryption keys, service-account token, SQLite data/mount, Caddy state and existing images/backups. Routine updates never rerun hardening.

Each attempt records `deployments/attempt-<random>/receipt.json`, with desired, running and last verified identities kept separate. `.website-verified.json` is the commit point after HTTP success and exact health/version verification; `.website-version` is a compatibility projection. A crash between their writes fails closed on the next attempt: inspect the receipt and running health before repairing the projection. SIGKILL/power loss can leave an unfinished receipt; no script automatically restores data or rolls an image back. Earlier servers without verified-image metadata record that identity as unknown on adoption.

Manual isolated rehearsals may set an absolute `MONOCHANGE_DEPLOY_DIR`, absolute executable `MONOCHANGE_BACKUP_HELPER` and `MONOCHANGE_HEALTH_TIMEOUT` (1..120 seconds). CI clears all three and fixes PATH. Never set these on production CI.

### Repeatable Linux verification

Run the fixed-path suite only in a newly owned throwaway container. It installs unmodified helpers into `/usr/local/bin` and owns `/opt/monochange`; never enable `ROLLOUT_FIXED_PATHS=1` on a real server. With an existing local Podman engine:

```bash
name="rollout-tests-$(date +%s)-$$"
podman run -d --name "$name" -v "$PWD:/workspace" -w /workspace node:24-bookworm sleep infinity
podman exec "$name" sh -c 'apt-get update && apt-get install -y sqlite3 jq curl util-linux && npm install --prefix /tmp/rollout-tools vitest@4.1.5'
podman exec -e ROLLOUT_FIXED_PATHS=1 "$name" /tmp/rollout-tools/node_modules/.bin/vitest run scripts/npm/tests/digitalocean-rollout.test.ts
podman rm -f "$name"
```

CI can use the same commands with its existing container engine. The suite uses file-backed Docker boundary fixtures for controlled failure injection, a real HTTP server/curl, real GNU flock and real SQLite. For real Docker load/Compose proof, build the text fixture in `fixtures/tests/digitalocean-rollout/`, tag it `monochange-app:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa`, and stream its Docker archive through gzip into `engine-rehearsal.sh` in a separate throwaway container with Docker CLI/Compose, jq, sqlite3 and the existing engine socket. That container must reach the engine's loopback port 3000, have an empty `/opt/monochange`, and mount this checkout at `/workspace`. Reserve port 3000 first; never stop another service to free it. The script removes only its named Compose app/network and copies receipts to `.monochange/local/rollout-repair/engine/`. Remove the owned helper container and fixture image afterwards. This proves the transaction with a fixture image; production linux/amd64 image validation remains a CI gate.
