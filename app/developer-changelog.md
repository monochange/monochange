# Developer updates for monochange.dev

Release notes for website developers and operators.

## 0.2.4

### 🚀 Feature

#### Use GitHub App user authorization for repository onboarding

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #782](https://github.com/monochange/monochange/pull/782)

Configure the GitHub App callback as `https://monochange.dev/auth/callback` and store its client ID and client secret in the `monochange_app` production profile. Login now uses PKCE and an exact callback URI, installation state is bound to the initiating browser, and expiring GitHub user tokens are refreshed from credentials stored by the new database migration.

### 🐛 Fixed

- **Document the production GitHub App configuration.** The deployment guide now records the GitHub App permissions, webhook events, OAuth boundary, installation flow and production secret locations required to connect repositories and create commits safely. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #777](https://github.com/monochange/monochange/pull/777)
- **Refresh shared ecosystem docs for Yarn lockfile support.** The shared documentation blocks on the website's guide pages and readme consumers now list Yarn workspaces and lockfiles alongside npm, pnpm, and Bun. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #778](https://github.com/monochange/monochange/pull/778) · _Closed issues:_ [#772](https://github.com/monochange/monochange/issues/772)

<details>
<summary><strong>🔨 Refactor</strong></summary>

#### Load monochange.dev secrets through Monosecret from one 1Password item

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #791](https://github.com/monochange/monochange/pull/791)

Production startup now reads every credential from a single 1Password item with one batched read, instead of one SecretSpec item per secret. This keeps the app well under the 1Password service-account request quota. Before deploying this release, operators must copy the production values into the new item and install the new Compose file.

`app/secretspec.toml` is replaced by `app/monosecret.toml`. The container selects its profile with `MONOSECRET_PROFILE` and no longer sets a provider:

```yaml
# before
environment:
  SECRETSPEC_PROFILE: production
  SECRETSPEC_PROVIDER: onepassword://monochange

# after
environment:
  MONOSECRET_PROFILE: production
```

The production profile reads the `monochange.dev` item in the `monochange` vault. Each secret is a field labelled with its name, in a section named after its group:

| Section   | Fields                                                                                                             |
| --------- | ------------------------------------------------------------------------------------------------------------------ |
| `auth`    | `JWT_SECRET`                                                                                                       |
| `github`  | `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, `GITHUB_APP_WEBHOOK_SECRET` |
| `release` | `MONOCHANGE_OIDC_AUDIENCE` (optional)                                                                              |
| `ai`      | `OPENROUTER_API_KEY` (optional)                                                                                    |

`DATABASE_URL` now comes from the Compose environment, and the bootstrap `OP_SERVICE_ACCOUNT_TOKEN` still comes from the Docker secret. The image keeps the bundled `op` CLI because Monosecret's 1Password provider shells out to it.

To migrate production:

1. Create the `monochange.dev` item with the sections and fields above, copying each value from its `secretspec/monochange_app/production/<KEY>` item.
2. Install `app/deploy/digitalocean/docker-compose.yml` from this release on the server before the image changes. The new image would treat a leftover `SECRETSPEC_PROVIDER` as an override and fail to start.
3. Deploy, then check `/health`, the app logs, sign-in, and the repository connection link.
4. Delete the old `secretspec/monochange_app/production/<KEY>` items.

Local development uses the `development` profile, which reads the ignored `app/.env`, then the environment, then local defaults, and never contacts 1Password. See "production cutover from SecretSpec to Monosecret" in `app/DEPLOY.md` for the full procedure and rollback.

</details>

## 0.2.3

### 🐛 Fixed

- **Allow release finalization to finish its cache cleanup.** Increase `release-post-merge.timeout-minutes` in `.github/workflows/ci.yml` from `10` to `20`. Pinned tooling setup, CLI compilation, release operations, and cache cleanup share this job deadline. The previous limit canceled release finalization while saving the Rust cache, after tags, draft releases, and the publish dispatch had succeeded. The larger budget leaves time for cleanup without changing release commands, permissions, or deployment checks. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #767](https://github.com/monochange/monochange/pull/767)

#### Use the maintainer identity for automated release pull requests

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #766](https://github.com/monochange/monochange/pull/766)

The repository's release workflow now opens release pull requests with the same existing maintainer token used to prepare their commits. Previously, the separate bot token made GitHub choose `github-actions[bot]` as the author of the queued squash commit and add the maintainer as a co-author.

The workflow now uses `RELEASE_PR_MERGE_TOKEN` for both operations. No new credential or permission is required. Existing bot-authored release pull requests must be replaced under the maintainer account before merging when sole maintainer authorship is required; changing the token does not change their author.

- **Verify repository changes through the dashboard.** The native SSR regression suite now delivers signed GitHub installation and repository-selection webhooks through the real API router, reloads the dashboard, and verifies additions, removals, suspension, uninstall and isolation between accounts. A second journey checks that organization repositories disappear when the installer loses ownership, and expired GitHub authorization shows a retry/sign-in state instead of an empty workspace. Only GitHub's external responses are replaced; cookies, SQLite, webhook signature verification and rendering stay real. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #771](https://github.com/monochange/monochange/pull/771)

## 0.2.2

### 🐛 Fixed

#### Choose release-note streams by audience for every package type

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #762](https://github.com/monochange/monochange/pull/762)

Generated release-agent instructions and the bundled skill now require agents to read configured stream and type descriptions and inspect output destinations before writing changesets. Application packages can use developer types for deployment or CI changes and product types for visible behavior. Agents create separate notes for the same package only when both audiences need them, then preview and render each output for review.

Use `monochange subagents <target> --force` to refresh an existing generated agent definition. Review local edits before replacing it. Inspect each audience with `monochange notes --output <id> --target <package>`; notes with a selected stream still need human or agent review of their prose.

The website now retains operational notes in `app/developer-changelog.md` instead of putting them in its public feed. Its implicit default changelog remains disabled. A named default-stream output retains those entries alongside the existing website outputs:

```toml
[changelog.outputs.website_developer]
stream = "default"
targets = ["monochange_app"]
path = "app/developer-changelog.md"
format = "keep_a_changelog"
mode = "append"
```

- **Resolve production deployment credentials in the environment-bound job.** The website deployment job now runs directly in `ci.yml` with the `website-production` environment, so its SSH key reaches the deployment step without crossing a reusable-workflow secret boundary. The job validates key presence and parsing before building the update and removes the temporary key after use. Operators receive an early error when deployment credentials are missing or malformed. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #759](https://github.com/monochange/monochange/pull/759)
