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

## API credential

Create a custom-scoped DigitalOcean token named `monochange deployment`, expiring after 90 days. Use `account:read`, `actions:read`, `droplet:read`, `domain:create`, `domain:read`, `domain:update`, `firewall:create`, `firewall:read`, and `firewall:update`. DigitalOcean adds the required read scopes for regions, sizes, images, and snapshots. These are resource-type scopes across the team; they are not restricted to one Droplet. Creating or deleting Droplets, deleting DNS/firewalls, billing, registries, and app-platform access are outside this token's permissions.

Store the generated token using Monosecret's secure prompt, never as a command argument:

```nu
ms set --file app/deploy/digitalocean/monosecret.toml --reason "store DigitalOcean deployment token" DIGITALOCEAN_ACCESS_TOKEN
```

The deployment manifest addresses the local system keyring under project `monochange_deployment`. It resolves only this API token, does not use the shared development service account, and does not read the app's runtime secrets. Keep a recovery copy and its expiry in a separate private `monochange deployment` 1Password vault. Do not put operational credentials in the runtime `monochange` vault: the app's service account must not be able to administer its host.

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

`doctl` reads `DIGITALOCEAN_ACCESS_TOKEN` from the child environment. Do not run `doctl auth init`, put tokens in shell history, print the environment, or retry failed secret resolution. After expiry, deliberately replace the keyring value and recovery copy with a new token of the same scope.

The API reference is [DigitalOcean's official API documentation](https://docs.digitalocean.com/reference/api/reference/); CLI commands are documented in the [doctl reference](https://docs.digitalocean.com/reference/doctl/reference/).

## SSH checks and updates

SSH does not require the DigitalOcean token. Inside `devenv shell`:

```nu
ssh -F app/deploy/digitalocean/ssh_config monochange-production 'id; cd /opt/monochange && docker compose ps'
```

The `deploy` user belongs to Docker's group and therefore has effective root access. Use the existing personal key for local maintenance. Never copy that private key to GitHub Actions. A future CI key must be a separate credential with its own grant and rotation policy.

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

Update the Compose and Caddy files from the same revision. These public configuration files must be owned by `deploy`, as installed by the bootstrap guide, so ordinary updates can replace them. The secrets directory and service-account token remain root-owned. Back up SQLite before restarting:

```nu
scp -F app/deploy/digitalocean/ssh_config app/deploy/digitalocean/docker-compose.yml app/deploy/digitalocean/Caddyfile monochange-production:/opt/monochange/
ssh -F app/deploy/digitalocean/ssh_config monochange-production '/usr/local/bin/monochange-sqlite-backup'
ssh -F app/deploy/digitalocean/ssh_config monochange-production $"/usr/local/bin/monochange-deploy monochange-app:($revision)"
curl --fail --silent --show-error https://monochange.dev/health
```

The server deploy helper requires a full commit SHA, verifies the image exists locally, persists its tag in `/opt/monochange/.env`, starts Compose without a registry pull, and waits for HTTP health. On failure, inspect the deployment before another restart; do not keep restarting a service with missing production secrets. Schema migrations may make an automatic image rollback unsafe. Restore a consistent backup deliberately when database compatibility requires it.

Deploy helpers are installed by `harden-droplet.sh`. For an already hardened server, update the helper separately rather than rerunning account/firewall bootstrapping. Production secrets remain in the runtime vault and the root-owned token file; ordinary updates do not copy or resolve them locally.

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

## initial setup still required

The API token must be created and stored before authenticated API commands work. The runtime 1Password service account, production secrets, and GitHub OAuth/App credentials must be completed before the first public deployment. Local daily backups exist; offsite backup delivery is still pending. Keep this checklist current as each setup step is verified.
