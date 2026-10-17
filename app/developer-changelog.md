# Developer updates for monochange.dev

Release notes for website developers and operators.

## 0.3.0

### 💥 Breaking Change

#### Remove the placeholder roadmap and feedback server functions

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #786](https://github.com/monochange/monochange/pull/786)

**Breaking change:** the placeholder server functions that predate project feedback are removed from `monochange_app`, along with their HTTP endpoints:

- `server_fns::roadmap::list_roadmap`, `server_fns::roadmap::vote_roadmap_item`, and `server_fns::roadmap::RoadmapItem` (the whole `server_fns::roadmap` module);
- `server_fns::feedback::submit_feedback` and `server_fns::feedback::FeedbackSubmission`.

They returned empty results and nothing called them. Use the project feedback server functions instead: `portal_view`, `share_feedback`, and `portal_vote` in `server_fns::portal` for the public portal, and `feedback_console`, `feedback_action`, and `maintainer_reply` in `server_fns::feedback` for maintainers.

```rust
// before
let items = server_fns::roadmap::list_roadmap(repo_id).await?;

// after
let snapshot = server_fns::portal::portal_view(organization, project).await?;
let roadmap = snapshot.map(|snapshot| snapshot.feed.roadmap);
```

No migration guide applies: these functions were only reachable from the website's own pages, which the same release replaces.

### 🚀 Feature

#### Advance project feedback from pull request and release webhooks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #788](https://github.com/monochange/monochange/pull/788) · _Closed issues:_ [#12](https://github.com/monochange/monochange/issues/12)

`POST /api/github/webhooks` now handles `pull_request` and `release` events for every project that includes the event's repository:

| Event                                            | Effect                                                                                                                                                                                                                                          |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `pull_request` `opened`, `reopened`, or `edited` | `FeedbackService::observe_pull_request` moves accepted or building items to review when the body closes their issue in the same repository (`Fixes #12`, `Fixes owner/name#12`, or an issue URL) or names them with a `Feedback-Item:` trailer. |
| `pull_request` `closed` with `merged: true`      | Calls `observe_merge`.                                                                                                                                                                                                                          |
| `release` `published` (not prereleases)          | Calls `ship_merged`. The version is the tag without a path prefix or leading `v` (`web/v1.2.0` → `1.2.0`); the notes link is the release page.                                                                                                  |

New public API:

- `monochange_app_feedback::closing_issue_numbers` and `feedback_trailers` parse pull request bodies.
- `IssueRef::is_in` checks an issue's repository.
- `monochange_app_api::feedback::Delivery` and `observe_delivery` apply an event to each affected project.

`monochange_app_api::feedback::update` no longer saves when an operation leaves the state unchanged and produces no notifications, so unrelated webhook traffic doesn't bump document versions.

Existing GitHub App registrations must subscribe to the `release` event. No new permission is required.

#### Use GitHub App user authorization for repository onboarding

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #782](https://github.com/monochange/monochange/pull/782)

Configure the GitHub App callback as `https://monochange.dev/auth/callback` and store its client ID and client secret in the `monochange_app` production profile. Login now uses PKCE and an exact callback URI, installation state is bound to the initiating browser, and expiring GitHub user tokens are refreshed from credentials stored by the new database migration.

#### Store organisations and projects for connected repositories

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #785](https://github.com/monochange/monochange/pull/785)

Database migration `004_add_organizations_and_projects` adds `provider` to `users`, `organizations`, `installations`, and `repositories`, adds `installations.organization_id` and `organizations.account_type`, and creates `projects` and `project_repositories`. Project repositories are keyed by `(provider, repository_external_id)` instead of `repositories.id`, so projects survive an uninstall and reinstall. Rolling the migration back leaves `installations.organization_id` in place because SQLite cannot drop a foreign-key column.

`installation` and `installation_repositories` webhooks now upsert the installation's account into `organizations` through `monochange_app_db::projects::link_installation_organization`. Installations recorded earlier are linked on first visit through the new `GitHubAppAuth::installation_account`, which reads `GET /app/installations/{id}` with the app JWT.

New server functions in `monochange_app::server_fns::organizations` (`list_organizations`, `organization_overview`, `create_project`, `project_overview`, `update_project`, `delete_project`) derive access from the repositories a user can manage, reusing the existing organisation-owner check. Routes `/dashboard/{organization}` and `/dashboard/{organization}/projects/{project}` render them; project forms use plain `ActionForm` fields such as `repositories[]`, so they work before hydration.

#### Store project feedback and expose the console and portal

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #786](https://github.com/monochange/monochange/pull/786)

Adds the `monochange_app_feedback` crate: the feedback pipeline with intake validation, prompt-injection screening, a `TriageEngine` seam with `RuleBasedTriage`, discussion and re-triage, voting, maintainer-only decisions, duplicate folding, and notifications. It also includes the disclosure rules (`Sensitivity::allowed_on(Surface, RepositoryVisibility)`) and the GitHub handoff drafts. `FeedbackService::from_state` and `state` round-trip a project's `FeedbackState`. Issue and pull request references now carry their repository, so merges and releases match by repository as well as number (`observe_merge(repository, number)` and `ship_merged`).

Database migration `005_create_project_feedback` adds `project_feedback`, one versioned JSON document per project, and `feedback_notifications`. `monochange_app_db::feedback::save_feedback` uses optimistic concurrency, and `monochange_app_api::feedback::update` reruns an operation on a lost race, up to three attempts. `monochange_app_api::feedback::create_issue` opens an issue through the GitHub App installation token with the new `github_app::create_issue`, then links it.

New server functions: `feedback_console`, `feedback_action`, and `maintainer_reply` for maintainers, plus `portal_view`, `share_feedback`, `portal_vote`, `portal_reply`, `similar_requests`, and `preview_markdown` for the public portal. Visitors are identified by a random `__Host-monochange_viewer` cookie, mapped to a per-project pseudonymous id with `feedback::viewer_id`. A project with a private or disconnected repository is treated as private.

### 🐛 Fixed

- **Document the production GitHub App configuration.** The deployment guide now records the GitHub App permissions, webhook events, OAuth boundary, installation flow and production secret locations required to connect repositories and create commits safely. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #777](https://github.com/monochange/monochange/pull/777)

#### Resolve the hosted release API token through Monosecret

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #789](https://github.com/monochange/monochange/pull/789)

The hosted release endpoints (`POST /api/release-commits` and `POST /api/release-requests`) read `MONOCHANGE_TOKEN` straight from the process environment. It was the only release secret that bypassed the app's secret manifest, so production never supplied it from 1Password.

`MONOCHANGE_TOKEN` is now an optional password entry in `app/monosecret.toml`. The production profile reads it from the `release` section of the `monochange.dev` item, and it is loaded into `AppState::api_token`; an empty value counts as unset. Behavior is unchanged: tokens are compared in constant time, an unconfigured token answers `503`, and a wrong token answers `401`.

Operators who set `MONOCHANGE_TOKEN` as a plain environment variable on the server must add it as a `MONOCHANGE_TOKEN` password field in the `release` section of the `monochange.dev` item instead. Deployments that rely only on GitHub Actions OIDC need no change.

#### Refresh existing release branches from hosted release commits

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #789](https://github.com/monochange/monochange/pull/789)

`POST /api/release-commits` returned `409 Conflict` whenever the release branch existed at any commit other than the run's base commit. After the first run the branch always holds the previous release commit, so every later push failed until the release pull request was merged and its branch deleted.

The release branch is rebuilt from the base branch on every run, so the endpoint now replaces it with a forced ref update. Creating the branch or fast-forwarding it from `base_commit` is always allowed. A forced replacement is allowed only when all of these hold:

- the request names `base_branch` (CLIs before this release do not);
- the branch has the release-branch shape `<branch_prefix>/release`;
- the branch is neither `base_branch` nor the repository's default branch (read from `GET /repos/{owner}/{repo}` only when a force is needed);
- `base_branch` still points at `base_commit`.

When the base branch has moved, the run is stale and the endpoint returns `409` with a `base branch ... moved` message, leaving the release to the run for the newer commit. Any other refused replacement returns `409` with a `release branch ... moved` message, so a caller cannot force `main` or another non-release branch onto a different commit. The branch check runs before any blob, tree, or commit is written.

Both hosted endpoints now reject branch names that are not plausible Git refs with `400`: empty names, `..`, `//`, `@{`, a leading `/` or `-`, a trailing `/`, `.`, or `.lock`, control characters, and characters that Git forbids or that would change the API URL (`space ~ ^ : ? * [ \ # %`).

The endpoint also reported `verified: true` for every commit; it now returns GitHub's verification result.

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

- **Readability formatting across the workspace.** Apply monostyle's formatter-safe whitespace conventions and dprint normalization across every crate — blank-line separation around control flow and consistent formatting only, with no behavior changes. The repository now scores 84.5 overall on monostyle (readability 89.6) and gates future changes through `lint:monostyle`. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #793](https://github.com/monochange/monochange/pull/793)

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
