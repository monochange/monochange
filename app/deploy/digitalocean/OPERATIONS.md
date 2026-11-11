# production CLI operations

Run these commands from the repository root. `devenv shell` supplies DigitalOcean's official `doctl` CLI and OpenSSH. App updates use SSH; the API manages cloud resources. No registry publication is needed.

## production target

| Resource         | Value                                                     |
| ---------------- | --------------------------------------------------------- |
| Droplet          | `606211709` (`monochange-app`)                            |
| IPv4             | `167.172.51.148`                                          |
| Region / size    | `lon1` / `s-1vcpu-1gb`                                    |
| Cloud firewall   | `1932d940-ca0c-49c2-b27c-08a0b464f31a` (`monochange-app`) |
| Site             | `https://monochange.dev`                                  |
| SSH user         | `deploy`                                                  |
| Deploy directory | `/opt/monochange`                                         |

The committed `ssh_config` uses the existing personal SSH key and refuses interactive authentication or unknown host keys. Its ed25519 host key was verified through the authenticated DigitalOcean console: `SHA256:IDrE9qR3H3uh1KYauCGDRPGjeEj8pbtXfrmgCMHC6aE`. A replacement Droplet needs a newly verified host key and updated target; never bypass this check.

## Automated website releases

The `website-deployment` job in `.github/workflows/app.yml` runs after an approved release commit contains a `monochange_app` release target. [How a release reaches production](../../DEPLOY.md#how-a-release-reaches-production) walks through each step, and [recover a failed deployment](../../DEPLOY.md#recover-a-failed-deployment) covers re-runs and rollbacks. It reads the dedicated deployment key directly from the `website-production` environment and validates the key before tooling setup or image building. Library-only releases leave the website alone. The deployment validates the release record and builds the exact tagged commit on GitHub Actions, so the small Droplet only loads the image and restarts the application. The temporary key is removed even if an earlier step fails.

`harden-droplet.sh` installs `ci-deploy.sh` as root-owned `/usr/local/bin/monochange-ci-deploy` with mode 0755; on an existing server, follow [update the server helpers](../../DEPLOY.md#update-the-server-helpers). Add the dedicated key to the deploy user's `authorized_keys` with these options:

```text
restrict,command="/usr/local/bin/monochange-ci-deploy" ssh-ed25519 <public-key> monochange-website-actions
```

The key accepts only `deploy <full-commit-sha> <stable-version>` with a compressed image on stdin. It cannot request a shell, port forwarding, or a PTY. The forced command locks deployment, rejects an older version, loads the image, verifies its commit tag, backs up SQLite, and calls the existing deploy helper. Treat this key as production access: deployed application code can access runtime secrets through the container.

Store its private key as the `WEBSITE_DEPLOY_SSH_KEY` secret in the `website-production` Actions environment, restricted to the `main` branch, and keep a recovery copy as the `WEBSITE_DEPLOY_SSH_KEY` field of the `deploy` item in the `monochange` vault. Do not copy the personal SSH key into CI. Rotation means installing a new dedicated public key, replacing both stored copies, verifying deployment, and removing the old authorized key.

No API token, registry credential, DNS change, or additional paid service is required. Runtime passwords remain on the server. A failed build leaves production untouched; a failed rollout keeps the release draft. Inspect Actions and the current database before retrying or restoring a backup.

## API credential

Create a custom-scoped DigitalOcean token named `monochange deployment`, expiring after 90 days. Use `account:read`, `actions:read`, `droplet:read`, `domain:create`, `domain:read`, `domain:update`, `firewall:create`, `firewall:read`, and `firewall:update`. DigitalOcean adds the required read scopes for regions, sizes, images, and snapshots. These are resource-type scopes across the team; they are not restricted to one Droplet. Creating or deleting Droplets, deleting DNS/firewalls, billing, registries, and app-platform access are outside this token's permissions.

Store the generated token in the 1Password app, never as a command argument: add a password field labelled `DIGITALOCEAN_ACCESS_TOKEN` to the `deploy` item in the `monochange` vault of the Kickjump account, and note its expiry on the item.

The deployment manifest resolves only this API token, through your own signed-in 1Password app rather than the production service account, and does not read the app's runtime secrets. Every monochange credential lives in that one vault; see the [production secret layout](../../DEPLOY.md#production-secret-layout). The production service account can read the `deploy` item too, which is why the token stays narrowly scoped and expires after 90 days.

For a single API operation:

```nu
ms run --file app/deploy/digitalocean/monosecret.toml --reason "inspect production Droplet" -- devenv shell doctl compute droplet get 606211709
```

For several operations, resolve once into a shell and exit that shell afterwards:

```nu
ms run --file app/deploy/digitalocean/monosecret.toml --reason "maintain monochange production infrastructure" -- devenv shell
doctl compute droplet get 606211709
doctl compute firewall get 1932d940-ca0c-49c2-b27c-08a0b464f31a
```

`doctl` reads `DIGITALOCEAN_ACCESS_TOKEN` from the child environment. Do not run `doctl auth init`, put tokens in shell history, print the environment, or retry failed secret resolution. After expiry, deliberately replace the `DIGITALOCEAN_ACCESS_TOKEN` field with a new token of the same scope.

The API reference is [DigitalOcean's official API documentation](https://docs.digitalocean.com/reference/api/reference/); CLI commands are documented in the [doctl reference](https://docs.digitalocean.com/reference/doctl/reference/).

## SSH checks and updates

SSH does not require the DigitalOcean token. Inside `devenv shell`:

```nu
ssh -F app/deploy/digitalocean/ssh_config monochange-production 'id; cd /opt/monochange && docker compose ps'
```

The `deploy` user belongs to Docker's group and therefore has effective root access. Use the existing personal key for local maintenance. Never copy that private key to GitHub Actions; CI uses the separate forced-command key described in [automated website releases](#automated-website-releases).

For an update, first merge the deployment PR and confirm its Docker image smoke check passed. Select that exact commit. Build locally with a Docker engine targeting the Droplet's `linux/amd64` architecture:

```nu
let revision = (git rev-parse HEAD)
docker build --platform linux/amd64 -t $"monochange-app:($revision)" .
docker save $"monochange-app:($revision)" | gzip | ssh -F app/deploy/digitalocean/ssh_config monochange-production 'gunzip | docker load'
```

Alternatively, build the committed source on the Droplet so the laptop needs no Docker engine. This is slower on the 1 GiB server. Run only one build at a time and allow enough free disk space:

```nu
let revision = (git rev-parse HEAD)
git archive HEAD | ssh -F app/deploy/digitalocean/ssh_config monochange-production $"set -eu; mkdir -p /opt/monochange/source-($revision); tar -xf - -C /opt/monochange/source-($revision); docker build -t monochange-app:($revision) /opt/monochange/source-($revision)"
```

Update the Compose and Caddy files from the same revision. Automated releases never copy them, so compare them with the server copy whenever they change. These public configuration files must be owned by `deploy`, as installed by the bootstrap guide, so ordinary updates can replace them. The secrets directory and service-account token remain root-owned. Back up SQLite before restarting:

```nu
scp -F app/deploy/digitalocean/ssh_config app/deploy/digitalocean/docker-compose.yml app/deploy/digitalocean/Caddyfile monochange-production:/opt/monochange/
ssh -F app/deploy/digitalocean/ssh_config monochange-production '/usr/local/bin/monochange-sqlite-backup'
ssh -F app/deploy/digitalocean/ssh_config monochange-production $"/usr/local/bin/monochange-deploy monochange-app:($revision)"
curl --fail --silent --show-error https://monochange.dev/health
```

The server deploy helper requires a full commit SHA, verifies the image exists locally, writes its tag to `/opt/monochange/.env`, starts Compose without a registry pull, and waits up to 120 seconds for HTTP health, printing the app's startup logs if it never becomes healthy. It overwrites `.env` on every run; keep optional runtime settings in `/opt/monochange/app.env` as described in [optional settings in app.env](../../DEPLOY.md#optional-settings-in-appenv). On failure, inspect the deployment before another restart; do not keep restarting a service with missing production secrets. Schema migrations may make an automatic image rollback unsafe. Restore a consistent backup deliberately when database compatibility requires it.

Deploy helpers are installed by `harden-droplet.sh` from `monochange-deploy.sh` and `ci-deploy.sh` beside it. For an already hardened server, follow [update the server helpers](../../DEPLOY.md#update-the-server-helpers) rather than rerunning account/firewall bootstrapping. Production secrets remain in the runtime vault and the root-owned token file; ordinary updates do not copy or resolve them locally.

## OS updates and reboots

Ubuntu security updates are installed daily by `unattended-upgrades`. Docker and containerd come from Docker's apt repository, which is outside the allowed origins, so those need the manual package pass described below.

The Droplet also reboots itself when an unattended upgrade leaves `/var/run/reboot-required`, so a new kernel is never left installed but inactive:

| Setting         | Value                                                                     |
| --------------- | ------------------------------------------------------------------------- |
| Config file     | `/etc/apt/apt.conf.d/51unattended-upgrades-auto-reboot`                   |
| Reboot time     | `07:30` UTC, after the 06:00–07:00 `apt-daily-upgrade` window             |
| Logged-in users | A session does not block the reboot (`Automatic-Reboot-WithUsers "true"`) |

`harden-droplet.sh` writes that file, so a replacement Droplet gets the same behaviour. The reboot takes the site down for roughly 20–30 seconds; Compose brings the app and Caddy back because both use `restart: unless-stopped`.

Check the parsed settings, or change the window, with:

```bash
ssh -F app/deploy/digitalocean/ssh_config monochange-production 'apt-config dump | grep -i "Unattended-Upgrade::Automatic-Reboot"'
```

For a manual pass, run `apt-get update && apt-get -y dist-upgrade` as root, then reboot only when `/var/run/reboot-required` exists. Kernel updates require the reboot to take effect; Docker updates restart the daemon and containers on their own.

Avoiding downtime entirely needs either a second host behind a load balancer or Ubuntu Pro Livepatch, which applies supported kernel fixes without a reboot. Neither is part of this deployment yet.

## CVE-2026-98432 interim mitigation

Ubuntu 24.04 has not released a kernel with the CVE-2026-98432 fix yet. As of 2026-11-02, the [Ubuntu tracker](https://ubuntu.com/security/CVE-2026-98432) lists `linux` for noble as vulnerable with the fix in progress in `6.8.0-149.149`. The Droplet therefore shares the vulnerable host kernel with every container. Do not remove the mitigation until `uname -r` shows the fixed kernel and the Ubuntu page marks the noble `linux` package Released.

`mitigate-cve-2026-98432.sh` installs a persistent nftables table that drops inbound IPv4 TCP packets carrying IP options (`IHL > 5`) in `prerouting` at priority `-310`. The vulnerable kernel path is only reachable from a connection request with IP options. The prerouting hook covers both host services such as SSH and traffic DNATed to Docker-published Caddy ports; an input-only rule would miss Caddy.

It also installs `monochange-cve-2026-98432-mitigation.service`, which loads the table after a reboot. `harden-droplet.sh` installs `nftables` and calls the mitigation script, so a replacement Droplet gets the same rule.

Apply or reapply it on the running Droplet:

```bash
scp app/deploy/digitalocean/mitigate-cve-2026-98432.sh root@$DROPLET_IP:/root/
ssh root@$DROPLET_IP '/root/mitigate-cve-2026-98432.sh'
```

Verify the table and the unit:

```bash
ssh root@$DROPLET_IP 'nft list table inet cve_2026_98432'
ssh root@$DROPLET_IP 'systemctl status --no-pager monochange-cve-2026-98432-mitigation.service'
```

The `counter` on the rule reports matching packets. A non-zero count is expected on a public web host; it is not proof of exploitation.

Compose sets `GODEBUG=multipathtcp=0` on the Caddy container. Go 1.24+ enables MPTCP on listeners by default, and the demonstrated exploit requires an MPTCP listener. This removes that listener type for Caddy, but the underlying bug is not MPTCP-specific, so the nftables rule and kernel update are still required.

After the unattended upgrade installs the fixed kernel, the existing auto-reboot activates it. Confirm the running version, then remove the mitigation:

```bash
ssh root@$DROPLET_IP 'uname -r'
ssh root@$DROPLET_IP '/root/mitigate-cve-2026-98432.sh --remove'
```

`--remove` stops and disables the unit, deletes the table, and removes the unit, helper script, and rules file. Recheck `nft list tables` and `systemctl status` afterward.

## DNS through the API

DNS currently uses Namecheap's BasicDNS. DigitalOcean's API cannot change records there. Namecheap Email Forwarding is selected and its five MX records and SPF TXT record are active. That service depends on Namecheap DNS, so copying records alone does not preserve the forwarding service. Keep the current delegation until email forwarding is deliberately retired or replaced. Routine app deployments need no DNS changes.

If DNS is later moved to DigitalOcean for API management, perform a one-time migration:

1. Resolve the email-forwarding dependency, then read and preserve every Namecheap record, including mail, TXT/SPF, CAA, and any verification records.
2. Create the `monochange.dev` zone in DigitalOcean and recreate all records. Use `@` and `app` A records pointing at the Droplet and a `www` CNAME pointing at `monochange.dev.`.
3. Compare the new authoritative answers with the old zone before changing delegation.
4. Set Namecheap custom nameservers to `ns1.digitalocean.com`, `ns2.digitalocean.com`, and `ns3.digitalocean.com`. Coordinate DNSSEC if enabled, and verify public DNS and HTTPS after propagation.

After delegation, use the token-loaded shell:

```nu
doctl compute domain records list monochange.dev
doctl compute domain records update monochange.dev --record-id <record-id> --record-data <new-ip>
```

Use IDs from the list output and re-read the record after an update. Keep the zone migration and delegation change as explicit maintenance steps; creating an app image never edits DNS.

## Remaining setup

As of 2026-10-19, the `monochange deployment` API token exists in the `deploy` item and expires on 2027-01-17, so replace it in early January 2027. The GitHub App credentials in the `github` section are configured and verified, as of that date. Hosted workflow dispatch, new in `monochange_app` 0.5.0, also needs **Actions: Read and write**; confirm the App and its installation have it before that release (see the [registration checklist](../github-app-registration.md)). Local daily backups exist; offsite backup delivery is still pending. Routine website release deployment uses SSH and does not depend on the API token.
