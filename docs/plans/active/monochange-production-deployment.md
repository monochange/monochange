# monochange production deployment

Continue PR <https://github.com/monochange/monochange/pull/373> and deploy the app at `monochange.dev` using Chrome for account and infrastructure operations.

- [x] Create the $6/month LON1 Droplet and configure SSH, Docker, firewall rules, security updates, and consistent daily SQLite snapshots.
- [x] Point the root, `app`, and `www` DNS records at the Droplet.
- [x] Create the production vault and prepare a read-only service account scoped to it.
- [ ] Confirm the scoped account at creation time, then store its token in 1Password and the root-only server secret file.
- [ ] Finish the prepared GitHub OAuth and GitHub App registrations and store production secrets.
- [ ] Fix Docker dependencies and root-only secret mounting, typed bot credentials, OAuth state validation, secure cookies, Caddy configuration, and deployment instructions.
- [ ] Run app tests and static analysis, the deployment image smoke test, and the repository checks; keep PR 373 green and merge through the required queue.
- [ ] Deploy the verified image, check HTTPS, health, browser assets, sign-in, and installation behavior.
- [ ] Configure and verify an off-Droplet backup destination.

The app is not live yet. Production credentials and the final scoped access confirmation remain outstanding. The service account must have read access only to the `monochange` vault. Never reuse the shared development service account for production. No package publication, release workflow dispatch, tag changes, or release PR merges are part of this deployment.
