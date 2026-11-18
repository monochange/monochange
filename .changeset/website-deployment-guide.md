---
monochange_app: fix
---

# Keep operator settings across website deployments and explain failed rollouts

`/usr/local/bin/monochange-deploy` rewrites `/opt/monochange/.env` with the image tag on every rollout, so any setting stored there, such as `MONOCHANGE_FEEDBACK_MODEL`, disappeared at the next deployment. The Compose file now loads optional, non-secret runtime settings from `/opt/monochange/app.env`, which no deployment touches:

```yaml
services:
  app:
    env_file:
      - path: /opt/monochange/app.env
        required: false
```

`MONOCHANGE_FEEDBACK_MODEL` and `MONOCHANGE_FEEDBACK_COMPLETIONS_URL` are no longer interpolated from `.env`; set them in `app.env` instead. The app's defaults are unchanged.

The deploy helper moves out of `harden-droplet.sh` into `app/deploy/digitalocean/monochange-deploy.sh`, so the running server can be updated without rerunning the hardening. It now waits up to 120 seconds for `/health` instead of giving up after 20 attempts one second apart, and when the app never becomes healthy it prints `docker compose ps app` and the last 60 lines of the app's startup log, so the Actions log names the cause (for example a missing `TOKEN_ENCRYPTION_KEY`). `harden-droplet.sh` also installs `ci-deploy.sh` as `/usr/local/bin/monochange-ci-deploy` and refuses to run unless both helpers sit beside it.

Operators apply this once on the existing Droplet before the release that carries it deploys:

```bash
scp -F app/deploy/digitalocean/ssh_config app/deploy/digitalocean/docker-compose.yml monochange-production:/opt/monochange/
scp app/deploy/digitalocean/monochange-deploy.sh app/deploy/digitalocean/ci-deploy.sh root@167.172.51.148:/root/
ssh root@167.172.51.148 'install -o root -g root -m 0755 /root/monochange-deploy.sh /usr/local/bin/monochange-deploy \
  && install -o root -g root -m 0755 /root/ci-deploy.sh /usr/local/bin/monochange-ci-deploy \
  && rm /root/monochange-deploy.sh /root/ci-deploy.sh'
```

`app/DEPLOY.md` is reorganised around the release path: a release-day checklist, per-release operator notes (the required `TOKEN_ENCRYPTION_KEY` field from 0.4.0 and the GitHub App **Actions: Read and write** permission that hosted workflow dispatch needs), the automated `app.yml` deployment step by step, recovery and rollback for a failed rollout, and every optional runtime setting. The shared release-notes guide now names `app.yml`, not `ci.yml`, as the workflow that deploys the website.
