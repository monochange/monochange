# Developer updates for monochange.dev

Release notes for website developers and operators.

## 0.3.4

### 🐛 Fixed

#### Add canonical `monochange format` command

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #854](https://github.com/monochange/monochange/pull/854)

Adds `monochange format` to rewrite `monochange.toml` in the idiomatic layout: top-level sections in a fixed order, `[package.*]` and `[group.*]` tables sorted by id, group members sorted alphabetically, and `[cli.*]` workflows last. Comments stay attached to their tables and the document header stays at the top.

The command formats the result with taplo using the repository's tab-based, 100-column configuration, so it does not require a separate taplo installation. Use `monochange format --check` to fail CI when the file needs formatting without writing changes. The repository's `lint:monochange` task and CI lint job now run that check.

#### Encrypt GitHub OAuth tokens at rest and scope the hosted API token

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #834](https://github.com/monochange/monochange/pull/834)

Two changes to the monochange.dev database and hosted API:

- GitHub user access and refresh tokens are now sealed with AES-256-GCM before they are written to the SQLite database, under a new required `TOKEN_ENCRYPTION_KEY` secret (`auth` section for production, generated in development and CI). A database file disclosure no longer leaks live GitHub tokens. Rows written before this change stay readable in plaintext and are replaced with sealed values the next time the token refreshes; rotating the key invalidates stored tokens and users simply sign in again.
- The deployment-wide `MONOCHANGE_TOKEN` no longer authorizes every connected repository. It only acts on repositories listed in the new `MONOCHANGE_TOKEN_REPOSITORIES` environment variable (`owner/repo`, comma or whitespace separated, case-insensitive). Deployments that rely on the token must list the repositories it may act on, or use GitHub Actions OIDC tokens, which were already restricted per repository.

#### Tighten hosted endpoint verification and startup secret checks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #833](https://github.com/monochange/monochange/pull/833)

The hosted release API now rejects three edge cases that previously slipped through verification:

- GitHub Actions OIDC tokens must be signed with RS256. The verifier pinned the algorithm to the token header before, so a token that named any other algorithm selected how its own signature was checked.
- The server refuses to start when the resolved `JWT_SECRET` is an empty string. An empty secret is a valid HMAC key, and a profile that resolves the secret to nothing would have signed session tokens anyone could forge.
- The unreachable "repository mismatch" check after the connected-repository lookup is gone. Repositories are resolved by exact full name, so the check could never fire.

The constant-time comparison used for `MONOCHANGE_TOKEN` checks is now shared with the webhook signature path instead of existing as two identical copies.

#### Protect hosted release branches and pull requests

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #852](https://github.com/monochange/monochange/pull/852)

Enforce hosted release authorization before creating or moving any branch. Every hosted commit must name a `<prefix>/release` branch distinct from both the requested base and the repository default. Older clients may still create or fast-forward a valid release branch without naming a base; replacing an existing release branch requires a named base whose head still matches the prepared commit.

Hosted pull request updates now require a server-signed marker bound to the PR number, node ID, bot creator, repository IDs, head, and base. Requests cannot adopt an unrelated PR or enable auto-merge on it. Existing release PRs without a marker must be closed and recreated once; rotating the deployment JWT signing secret also requires recreating open release PRs. If GitHub creates a release PR but its marker update fails, close the newly created PR identified in the error before retrying.

- **Preserve workflows in hosted release commits.** Hosted release commits now compare the final `.github/workflows` subtree with the prepared base for both uploaded files and prebuilt trees. Requests fail if workflow contents change, a protected directory becomes a file, or a tree listing is incomplete. Uploaded file paths must be canonical repository-relative paths and cannot replace or delete `.github` or `.github/workflows`; uploads to other `.github` files remain supported. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #849](https://github.com/monochange/monochange/pull/849)

#### Bound OAuth work and reject callback replays

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #847](https://github.com/monochange/monochange/pull/847)

`/login` and `/auth/callback` now consume the anonymous address bucket before server rendering. Valid signed callback state is atomically consumed in SQLite before the provider exchange, so replaying the original cookie cannot trigger more exchanges, including after a process restart. Failed exchanges also consume their state; start a fresh sign-in attempt to retry.

The application runs migration `006_create_oauth_state_uses` on startup. Consumed states are retained until their signed expiry and pruned during subsequent callbacks. Provider token requests have a ten-second timeout, and each app process permits at most sixteen concurrent exchanges. Saturated requests can retry their existing state because capacity rejection happens before consumption. Existing rate-limit environment variables continue to control the address bucket.

#### Bound OIDC signing key refreshes independently of token key IDs

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #848](https://github.com/monochange/monochange/pull/848)

Authentication for `POST /api/release-commits` and `POST /api/release-requests` now rejects unknown signing key IDs from a fresh JWKS cache without contacting GitHub. The cache remains valid for ten minutes. Concurrent refreshes share one request, and a shared retry deadline of thirty seconds prevents failed or cancelled attempts from restarting once per key ID. Invalid tokens can no longer force one provider request per attempt.

A signing key first published by GitHub while the cache is fresh is not fetched or accepted until that ten-minute cache expires.

JWKS responses must have a successful HTTP status and fit within 64 KiB before parsing. Failed refreshes retain the previous key set but do not authorize tokens with expired keys. Deployments and workflow callers need no configuration changes.

#### Authorize organization access through stable provider identities

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #846](https://github.com/monochange/monochange/pull/846)

Bind dashboard organization and project authorization to each installation's stable provider account ID. GitHub membership responses must identify that same account before granting repository access, and legacy installations resolve their identity before membership checks. Organization logins remain display metadata; duplicate stored logins cannot select a tenant through public project routes. No configuration or data migration is required.

- **Clarify release-record validation in the CLI guide.** The `CommitRelease` reference now keeps mismatch protection enabled in its formatting example. It explains which prepared fields are checked, how rejected records preserve neighboring release records, and when deliberate recovery with `update_release_json = true` is appropriate. Whitespace and JSON key ordering require no configuration change. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #850](https://github.com/monochange/monochange/pull/850)

## 0.3.3

### 🐛 Fixed

#### Read GitHub OAuth token replies without discarding the error

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #828](https://github.com/monochange/monochange/pull/828)

`exchange_code` and the user-token refresh in `github_user_access_token` no longer call `reqwest::Response::json` on `POST /login/oauth/access_token`. They read the body as text and parse it in `parse_token_response`, which:

- returns `GitHub rejected the sign-in: <error_description> (<error>)` when GitHub sends an OAuth error instead of a token;
- accepts `application/x-www-form-urlencoded` replies and lifetimes sent as strings (`"expires_in": "28800"`);
- reports any other body as `GitHub returned an unreadable token response (HTTP <status>, <content type>): <title or first 200 characters>` and logs it with `tracing::warn!`.

Refresh failures still ask the user to sign in again, but the server log now carries GitHub's reason.

- **Document native package release setup.** The hosted ecosystem guide now explains how to configure Swift, CocoaPods, and Gradle packages, including versioned files and registry credentials. Follow the native ecosystem sections when adding Apple or Android packages to an existing workspace; Gradle and CocoaPods publishing require `trusted_publishing = false` and their registry credentials. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #830](https://github.com/monochange/monochange/pull/830)

## 0.3.2

### 🚀 Feature

- **Commit release trees built by the workflow.** `POST /api/release-commits` accepts a `tree` instead of `files`. The server checks that the request names exactly one of them and that `tree` is a git object id, then creates the release commit on that tree without uploading any blobs. A tree whose `.github/workflows` differs from the base commit's is refused with `422 Unprocessable Entity` before anything is written, keeping the rule that the bot never changes GitHub workflows. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #812](https://github.com/monochange/monochange/pull/812)

#### Report installations that haven't approved the app's new permissions

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #819](https://github.com/monochange/monochange/pull/819)

GitHub keeps each GitHub App installation on the permissions it approved until an owner of the account accepts the app's newer request. The new `permission_updates` server function compares the app registration's permissions (`GET /app`) with each of the signed-in user's installations (`GET /app/installations/{id}`) and returns the accounts that are behind, the missing permissions, and the GitHub settings page where an owner reviews the request. The dashboard shows them in a notice above the repository connection panel.

`monochange_app_api::github_app` gains `GitHubAppAuth::requested_permissions`, `GitHubAppAuth::installation_permissions`, and `pending_permissions`, which lists permissions the app requests above the installation's level:

```rust
let requested = app.requested_permissions(&http).await?;
let granted = app.installation_permissions(&http, installation_id).await?;
let missing = monochange_app_api::github_app::pending_permissions(&requested, &granted);
```

A failed GitHub lookup leaves that account out of the list instead of failing the dashboard. No configuration changes are needed.

### 🐛 Fixed

- **Send a User-Agent when the app calls the GitHub API.** Hosted release commits failed with `502` and "failed to mint installation token" because `POST /app/installations/{id}/access_tokens` went out without a `User-Agent` header, which GitHub rejects with `403 Forbidden`. The shared `reqwest::Client` built in `AppState::new` now sets `User-Agent: monochange`, so installation token, tree, commit, ref and pull request calls all identify the app. No configuration change is needed; redeploy monochange.dev to pick it up. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #818](https://github.com/monochange/monochange/pull/818)

#### Document how command output is ordered

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #813](https://github.com/monochange/monochange/pull/813)

The progress output reference now explains the order every multi-line command follows: the plan, its details, the log, the detailed results, and a closing summary line. It includes a placeholder publish example and lists the new `publish_plan` JSON progress event.

The `PublishPackages` and `PlaceholderPublish` step references now say that results end with their summary line, and describe how a registry rate limit is reported and resumed.

#### Check hosted release credentials before reading the request body

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #810](https://github.com/monochange/monochange/pull/810)

`POST /api/release-commits` and `POST /api/release-requests` now verify the `Authorization` bearer token (GitHub Actions OIDC or `MONOCHANGE_TOKEN`) before reading the request body. A caller without valid credentials gets `401` without the server buffering its payload. Previously the JSON body, up to 32 MiB for release commits, was read and parsed before authentication.

The release commit body limit drops from 32 MiB to 8 MiB, which still fits monochange's own grouped release (about 3 MiB). A body over the limit now returns `413 Payload Too Large` instead of `400`, and the release request endpoint keeps a 2 MiB limit. An OIDC token for one repository still gets `403` when the request targets another, now before the repository lookup.

`AppState` gains `oidc: Arc<OidcVerifier>`, so the GitHub Actions JWKS is cached across requests instead of being fetched on every call.

- **Recommend `monochange publish` commands in the docs.** The guides, CLI step reference, and readme now use `monochange publish packages`, `monochange publish readiness`, and `monochange publish placeholder` in examples and instructions, and say that the matching `monochange step` commands remain supported for compatibility. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #811](https://github.com/monochange/monochange/pull/811)

## 0.3.1

### 🚀 Feature

#### Rate limit the API per client

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #807](https://github.com/monochange/monochange/pull/807)

The new `monochange_app::rate_limit` module limits Leptos server functions and the REST endpoints under `/api` with [`governor`](https://docs.rs/governor) GCRA buckets. Requests with a valid session cookie count against the signed-in user's bucket (300 a minute, burst 100); every other request counts against its client address (60 a minute, burst 30), with IPv6 addresses grouped by their /64. Pages, `/pkg` assets, `/health`, and `/api/github/webhooks` are not limited.

`X-Forwarded-For` is read from the right and only when the peer is a loopback or private address, which is Caddy in the production deployment. The server now runs with `into_make_service_with_connect_info::<SocketAddr>()` so the middleware sees the peer address.

Limited responses carry `RateLimit-Policy` and `RateLimit` fields, and a `429 Too Many Requests` adds `Retry-After`. Server function rejections use the `ServerFnError` encoding (`ServerError|Too many requests. Try again in 12 seconds.`); REST rejections use the endpoints' `{"message": ...}` JSON body.

Override the limits with environment variables; a value that isn't a whole number above zero stops startup:

```bash
MONOCHANGE_RATE_LIMIT_ANONYMOUS_PER_MINUTE=60
MONOCHANGE_RATE_LIMIT_ANONYMOUS_BURST=30
MONOCHANGE_RATE_LIMIT_AUTHENTICATED_PER_MINUTE=300
MONOCHANGE_RATE_LIMIT_AUTHENTICATED_BURST=100
```

#### Filter and paginate repository lists

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #804](https://github.com/monochange/monochange/pull/804)

The new `monochange_app::repository_filter` module holds the dashboard's list rules: `RepositoryFilter` reads the `q` (name search) and `org` (account) query keys and builds links with `href`, `matches_search` matches a repository's name (or `owner/name` when the search contains `/`), and `repository_page` returns one `REPOSITORIES_PER_PAGE` (20) page with its position and page count.

The new `components::repository_list` module renders them: `RepositoryFilters` (a GET search form that updates the URL in place after hydration), `RepositoryPager` (one list's pages), and `RepositoryPicker` (project-form checkboxes with a name filter that only hides rows, so hidden selections still submit). `RepoInfo` now derives `PartialEq` and `Eq`.

```text
/dashboard?q=web&org=acme
```

### 🐛 Fixed

#### Accept hosted release commits larger than 2 MiB

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #803](https://github.com/monochange/monochange/pull/803)

`POST /api/release-commits` now accepts request bodies up to 32 MiB (`monochange_app_api::RELEASE_COMMIT_BODY_LIMIT`). Before this change, the route used axum's 2 MiB default, so a grouped release that rewrote many changelogs was rejected with `400 Bad Request` (`length limit exceeded`) before it was authenticated. monochange's own release commits already send about 3 MiB of changelog content.

Other API routes keep the 2 MiB default. No configuration change is needed; deploy the new app version before switching a large repository's release workflow to `commit_backend = "hosted"`.

- **Document pnpm trusted publishing.** The trusted-publishing and configuration guides now explain that pnpm workspaces publish with `pnpm publish`, including under trusted publishing, that this requires pnpm 11.1.3 or later on the publishing runner, and why monochange passes `--no-git-checks` to pnpm. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #809](https://github.com/monochange/monochange/pull/809)

## 0.3.0

### 💥 Breaking Change

#### Require GitHub sign-in to write on the feedback portal

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #798](https://github.com/monochange/monochange/pull/798)

**Breaking change:** `server_fns::auth::get_login_url` now takes `next: Option<String>`, and `server_fns::portal::VIEWER_COOKIE` is removed along with the `__Host-monochange_viewer` cookie. Callers pass `None` to keep the old behavior:

```rust
// before
let url = get_login_url().await?;

// after
let url = get_login_url(None).await?;
```

`share_feedback`, `portal_vote`, and `portal_reply` now refuse a visitor without a valid session ("Sign in with GitHub to share.", "… to vote.", "… to join the discussion."). `portal_view` stays public: a missing, expired, or forged session reads as signed out, and `PortalSnapshot` gains `viewer: Option<PortalViewer>` with the visitor's login and avatar.

A signed-in visitor's per-project id comes from the new `monochange_app_api::feedback::account_viewer_id(secret, provider, external_id, project_id)`, an HMAC of `"{provider}:{external_id}"`. One account votes once per project, ids can't be linked across projects, and stored feedback never names a GitHub login. Items shared under a cookie id keep their votes and stay listed, but their author no longer sees them under "Yours".

Sign-in can return to where it started. `get_login_url(next: Option<String>)` and `oauth::login_state_returning(secret, next)` sign the destination into the OAuth state cookie, `VerifiedOAuth::return_to` carries it back, and `exchange_code` redirects there instead of `/dashboard`. `oauth::return_path` accepts only same-site paths, so absolute, protocol-relative, and backslashed values, control characters, and paths over 512 bytes fall back to the dashboard:

```text
/login?next=%2Fp%2Facme%2Fpocketbook%23roadmap  → back to /p/acme/pocketbook#roadmap
/login?next=%2F%2Fevil.example                  → /dashboard
```

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

#### Share accepted feedback as screened contributor tasks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #801](https://github.com/monochange/monochange/pull/801)

Projects can opt into the experimental shared compute flow and share an accepted item as a task that contributors run with their own agents. The new `monochange_app_feedback::tasks` module holds the task model (`TaskDraft`, `TaskTarget`, `ContributorTask`, `PublicTask`, `TaskStatus`) and `screen_task`, the rules that run before a task lists. `FeedbackService` gains `set_shared_compute`, `task_repository`, `share_task`, `withdraw_task`, `open_tasks`, and `open_task`, and `FeedbackState` stores `shared_compute` and `tasks` (both default when an older document loads).

`monochange_app_api::feedback::share_task` pins a task to the tip of the issue repository's default branch through the new `github_app::default_branch_head`, and lists it for `TASK_LIFETIME` (14 days). Anyone can read open tasks without signing in:

```sh
curl https://monochange.dev/api/projects/acme/invoices/tasks
curl https://monochange.dev/api/projects/acme/invoices/tasks/task-1
```

The console calls the new `set_shared_compute`, `share_task`, and `withdraw_task` server functions.

**Breaking change:** `ConsoleView` gains `shared_compute`, and `ConsoleItem` gains `tasks` and `shareable`, so code that builds them with struct literals must set the new fields:

```rust
ConsoleItem {
	// ...existing fields
	tasks: Vec::new(),
	shareable: false,
}
```

#### Replace page components with lazy route types

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #796](https://github.com/monochange/monochange/pull/796)

The public page components (`BookPageView`, `ChangelogPage`, `DashboardPage`, `FeedbackConsolePage`, `InstallPage`, `OrganizationPage`, `PortalPage`, `PricingPage`, `ProjectPage`, and the private `AuthCallbackPage`) are gone. Each route is now a `LazyRoute` implementation (`BookRoute`, `ChangelogRoute`, `DashboardRoute`, `FeedbackConsoleRoute`, `InstallRoute`, `OrganizationRoute`, `PortalRoute`, `PricingRoute`, `ProjectRoute`, `AuthCallbackRoute`) whose `data()` creates the route's resources and whose `view()` renders them, referenced from `app.rs` through `Lazy::<Route>::new()`. Nothing outside this crate consumed the components; the website's rendered output is unchanged.

```rust
// Before
<Route path=path!("/dashboard") view=DashboardPage />

// After
#[lazy_route]
impl LazyRoute for DashboardRoute {
	fn data() -> Self { /* resources */ }
	fn view(this: Self) -> AnyView { /* renders this */ }
}

<Route path=path!("/dashboard") view={Lazy::<DashboardRoute>::new()} />
```

### 🚀 Feature

#### Build accepted feedback with the repository's agent

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #798](https://github.com/monochange/monochange/pull/798)

The maintainer console offers `ConsoleAction::BuildWithAgent` (`"build_with_agent"`) on accepted items whose GitHub issue names its repository. `monochange_app_api::feedback::start_agent_build(state, scope, item_id)` checks that the issue's repository has `AGENT_WORKFLOW` (`.github/workflows/monochange-feedback-build.yml`), sends it a `repository_dispatch` event named `AGENT_BUILD_EVENT` (`monochange-feedback-build`), and moves the item to building. A missing workflow returns the new `FeedbackError::AgentWorkflowMissing`, and a failed dispatch leaves the item accepted so it can be retried.

The event's `client_payload` is the new `monochange_app_feedback::BuildRequest`, ids only:

```json
{
	"item": "fb-12",
	"repository": "acme/web",
	"issue_number": 42,
	"branch": "fix/feedback-fb-12"
}
```

`github_app::repository_file_exists` and `github_app::dispatch_repository_event` are the new GitHub calls behind it; both use the Contents access hosted releases already need. The repository's workflow, which runs the agent with a read-only token and opens a draft pull request with the `Feedback-Item:` trailer from a separate job, is documented in the new "Build accepted feedback with an agent" guide.

#### Advance project feedback from pull request and release webhooks

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #788](https://github.com/monochange/monochange/pull/788)

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

#### Split the website client, compress the edge, and gate page performance

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #796](https://github.com/monochange/monochange/pull/796)

Release builds must now pass `cargo leptos build --release --split` (the Dockerfile and app CI do): every route except the landing and login views is a `#[lazy_route]` boundary, so the initial WASM download excludes dashboard, organisation, project, feedback, portal, book, changelog, install, and pricing code. Server functions referenced only by those views move into their route chunks, and the client hydrates through `leptos::mount::hydrate_lazy`, which lazy routes require.

Build and deployment changes:

- The WASM client compiles with the new `wasm-release` profile (`opt-level = "z"`, `panic = "abort"`, js-minify enabled) while the server binary stays on a runtime-first `release` profile (`opt-level = 3`).
- `strip` moved from the cargo profile into the Dockerfile: cargo's implicit `-C strip=debuginfo` corrupts proc-macro dylibs on newer macOS hosts, so profiles set `strip = "none"` and the image strips the server binary after linking. Proc-macros and build scripts build at `opt-level = 0` via `build-override`, which also speeds up cold builds.
- The Dockerfile uses BuildKit cache mounts for the cargo registry and target directory, so dependency artifacts survive between image builds.
- `/pkg` hydration assets are served `Cache-Control: public, max-age=0, must-revalidate` and the static file handler answers `If-Modified-Since` with 304s, so a deployment can never pair fresh HTML with stale client code while repeat visits revalidate for free. The image ships precompressed `.gz`/`.br` siblings of every pkg asset that the file handler serves to accepting clients, and the Caddy edge enables `encode zstd gzip` for everything else — production previously shipped every byte uncompressed.
- `app/deploy/verify-image.sh` also asserts the split build's manifest and that pkg assets are served gzip-compressed to accepting clients.

New website benchmarking, run by the `benchmark` job in `app-ci.yml` on every app-affecting change and locally through `pnpm bench:website` / `pnpm size:website`:

- `scripts/benchmark-website.ts` boots the release server against a throwaway SQLite database and runs Lighthouse (desktop preset) across the core flows, failing when metrics in `scripts/website-budgets.json` regress. `MONOCHANGE_ALLOW_BENCHMARK_BUDGET_FAILURES=true` keeps the CLI benchmark escape hatch.
- `scripts/website-size.ts` compares the gzip transfer size of `app/target/site/pkg` against the committed `scripts/website-size-baseline.json` and fails when the total grows beyond 1% (or a transferred file beyond 5%, absorbing cross-platform codegen variance) until the baseline is deliberately updated with `pnpm size:website:update`.

### 🐛 Fixed

- **Document the `--color` flag in the progress output reference.** The progress output reference page on the website now has a "Color" section that explains the global `--color <auto|always|never>` flag and the order in which `NO_COLOR`, `CLICOLOR_FORCE`, `CLICOLOR`, and `TERM=dumb` are checked under `auto`. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #795](https://github.com/monochange/monochange/pull/795)
- **Document the production GitHub App configuration.** The deployment guide now records the GitHub App permissions, webhook events, OAuth boundary, installation flow and production secret locations required to connect repositories and create commits safely. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #777](https://github.com/monochange/monochange/pull/777)

#### Warn at startup when the GitHub App is not configured

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #799](https://github.com/monochange/monochange/pull/799)

The app now logs a warning at startup when `GITHUB_APP_ID`, `GITHUB_APP_PRIVATE_KEY`, and `GITHUB_APP_WEBHOOK_SECRET` are all unset. Before this change, a deployment without these values started silently and the dashboard only reported "Repository connection is unavailable", so operators had no server-side signal pointing at the missing credentials.

Configuration is unchanged. To enable repository connection in production, add the three values as fields of the `github` section in the `monochange.dev` item of the `monochange` vault and restart the app:

```bash
docker compose restart app
docker compose logs --tail=100 app
```

- **Explain which issues `CommentReleasedIssues` links.** The `CommentReleasedIssues` reference page now has a "Which issues are linked" section. It explains that only numbers GitHub resolves to issues in the release repository are linked, that pull requests, other repositories and code are skipped, and what happens when the lookup fails. _Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #797](https://github.com/monochange/monochange/pull/797) · _Closed issues:_ [#745](https://github.com/monochange/monochange/issues/745)

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

#### Keep deployment credentials in the `monochange` 1Password vault

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #799](https://github.com/monochange/monochange/pull/799)

`app/deploy/digitalocean/monosecret.toml` now reads `DIGITALOCEAN_ACCESS_TOKEN` from the `deploy` item in the `monochange` vault, through the operator's own signed-in 1Password app. It no longer reads the local system keyring, and the separate `monochange deployment` recovery vault is retired. The same `deploy` item holds recovery copies of `WEBSITE_DEPLOY_SSH_KEY` and the read-only production `OP_SERVICE_ACCOUNT_TOKEN`, next to the app's `monochange.dev` runtime item, so every monochange credential lives in one vault.

Operators who kept the token in the keyring must add it to 1Password once:

| Item     | Field                       | Type     |
| -------- | --------------------------- | -------- |
| `deploy` | `DIGITALOCEAN_ACCESS_TOKEN` | password |

Commands are unchanged:

```bash
ms run --file app/deploy/digitalocean/monosecret.toml --reason "inspect production Droplet" -- devenv shell doctl compute droplet get 606211709
```

The app never reads the `deploy` item, but the production service account can, so keep the DigitalOcean token narrowly scoped and expiring. `app/DEPLOY.md` also now says the old SecretSpec items keep their value in a `password` field, not `value`.

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
