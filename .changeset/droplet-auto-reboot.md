---
monochange_app: fix
---

# Reboot the production Droplet after security updates

The DigitalOcean hardening script now writes `/etc/apt/apt.conf.d/51unattended-upgrades-auto-reboot`, so the Droplet reboots at 07:30 UTC when an unattended Ubuntu security upgrade leaves `/var/run/reboot-required`. The new kernel is activated instead of waiting for a manual reboot; Compose restarts the app and Caddy because both use `restart: unless-stopped`. The operations guide documents the window, the short outage, and the Docker repository updates that still need a manual pass.
