# DigitalOcean deployment assets

This directory holds the server-side files for the `monochange.dev` Droplet. `app/DEPLOY.md` explains when each one changes and how to install it.

| File                         | Installed as                                                                                   |
| ---------------------------- | ---------------------------------------------------------------------------------------------- |
| `docker-compose.yml`         | `/opt/monochange/docker-compose.yml`, copied by hand; automated releases never update it       |
| `Caddyfile`                  | `/opt/monochange/Caddyfile`, copied by hand                                                    |
| `harden-droplet.sh`          | Run once as root on a new Droplet, with the helper scripts beside it                           |
| `monochange-deploy.sh`       | `/usr/local/bin/monochange-deploy`, the health-checked rollout helper                          |
| `ci-deploy.sh`               | `/usr/local/bin/monochange-ci-deploy`, the forced command of the GitHub Actions deployment key |
| `mitigate-cve-2026-98432.sh` | The interim nftables mitigation and its systemd unit                                           |
| `create-firewall.sh`         | Run locally with `doctl` to create or attach the cloud firewall                                |
| `monosecret.toml`            | Run locally to resolve the DigitalOcean API token from the `deploy` 1Password item             |
| `ssh_config`, `known_hosts`  | Run locally: the pinned SSH target `monochange-production`                                     |

The production app loads secrets with the Monosecret Rust SDK from one 1Password item, `monochange.dev` in the `monochange` vault. Docker Compose only injects the bootstrap 1Password service account token as a Docker secret from `/opt/monochange/secrets/op_service_account_token`, and loads optional non-secret settings from `/opt/monochange/app.env`.

Persistent SQLite data is stored on the Droplet at `/opt/monochange/data` and mounted into the app container as `/data`. Day-to-day operations are in `OPERATIONS.md`.
