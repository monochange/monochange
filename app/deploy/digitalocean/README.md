# DigitalOcean deployment assets

This directory contains the Docker Compose and Caddy files used by `app/DEPLOY.md`.

The production app loads secrets with the Monosecret Rust SDK from one 1Password item, `monochange.dev` in the `monochange` vault. Docker Compose only injects the bootstrap 1Password service account token as a Docker secret from `/opt/monochange/secrets/op_service_account_token`.

Persistent SQLite data is stored on the Droplet at `/opt/monochange/data` and mounted into the app container as `/data`.

Read `app/DEPLOY.md` for the complete hardened Droplet, backup, and deploy procedure.
