# monochange.dev platform roadmap

**Status**: decided; phase 1 in progress **Related**: `docs/plans/active/monochange-app-planning.md`, `docs/plans/active/feedback-loop-poc.md`

## Goal

monochange.dev connects to the organisations where code lives and closes the loop from "a user wants something" to "it shipped":

1. Sign in with a source provider and bring in organisations (GitHub first, then every provider monochange supports: GitLab, Gitea, Forgejo).
2. monochange prepares releases for connected repositories: it generates the release commit and the release pull request, verified and attributable to monochange.
3. Maintainers create **projects** from an organisation. A project spans one or more of the organisation's repositories and has its own dashboard.
4. A project collects **feedback** from its users through a friendly editor, lets them vote, and keeps them informed until release.
5. With the owner's approval, **agents** plan and build the chosen feedback, report progress back to the user's original issue, and the release closes it.

## Where things stand

| Capability                                          | State                                                                                                                                                |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| GitHub sign-in and repository onboarding            | On `main` (#782): one GitHub App for login and installation, expiring user tokens, organisation-owner checks.                                        |
| Hosted release commit and release request endpoints | On `main`: OIDC-authenticated `/api/release-commits` and `/api/release-requests`, GitHub App bot identity, verified-commit enforcement.              |
| Release automation scheduler                        | On `main`: `monochange_app_automation` schedules and dispatches release jobs.                                                                        |
| `monochange/actions`                                | `release-pr`, `open-release-request`, `post-merge-release`, `merge`, … run the CLI in the repository's CI. None call the hosted endpoints yet.       |
| Feedback pipeline                                   | `monochange_app_feedback` on this branch: intake, screening, triage seam, discussion, voting, disclosure gate, GitHub handoff drafts, notifications. |
| Provider crates                                     | `monochange_github`, `monochange_gitlab`, `monochange_gitea`, `monochange_forgejo` already create release requests through each provider's API.      |
| Projects, organisations as first-class, GitLab app  | Not started.                                                                                                                                         |

## Phases

Each phase is one or more stacked pull requests that ship on their own.

### 1. Organisations and projects (GitHub)

- Make accounts provider-neutral now, so GitLab later is an adapter, not a migration: `provider` on users, installations, organisations, and repositories; identities keyed by `(provider, external_id)`.
- Organisations become the top level of the dashboard: each connected organisation lists its repositories and the installer's role.
- Projects: `projects` (organisation, name, slug, visibility), `project_repositories` (many repositories per project, same organisation), `project_members` (owner, maintainer). Only organisation owners create projects at first; the GitHub App's existing organisation-owner check gates it.
- Pages: `/dashboard/{org}`, `/dashboard/{org}/projects/new`, `/projects/{org}/{project}` (maintainer dashboard).

### 2. Hosted release commits and pull requests

- New `monochange/actions/hosted-release` action. In the repository's workflow it runs `monochange step prepare-release`, requests a GitHub OIDC token for the `monochange.dev` audience, and posts the prepared change to `/api/release-commits` and `/api/release-requests`. The commit and pull request are created by the monochange GitHub App, so they are verified and attributable without the repository holding a write token.
- App side: per-repository opt-in in the project dashboard, release activity history, and the existing automation scheduler offered as "release on a schedule".
- This phase is independent of 1 and can run in parallel.

### 3. Feedback on projects

- Move the feedback pipeline from repository scope to project scope. A project's disclosure policy is the strictest of its repositories; each accepted item targets one repository for its issue.
- Persistence: one versioned document per project plus a notifications table (designed and half-built on this branch).
- Maintainer console in the project dashboard; public project portal at `/p/{org}/{project}` with roadmap, voting, "your feedback", and status updates; embeddable widget script for apps (point at an element, screenshots, similar-request suggestions).
- **Editor.** Users write in a friendly editor that stores Markdown, so text maps one-to-one onto provider issues. A formatting toolbar, paste and drag-in screenshots (uploaded to object storage and referenced by id), and live preview. Recommended: a ProseMirror-based Markdown editor (such as Milkdown) loaded as a small client island, with server-side sanitisation through the existing `ammonia` dependency.
- Accepting an item creates the issue in the target repository through the GitHub App; pull request and release webhooks move the item forward.
- The `/feedback` page keeps the playground as a per-visitor sandbox so anyone can try the flow without connecting a repository.

### 4. Agents

- The maintainer marks an accepted item **next**. monochange asks an agent for a plan, posts it on the issue, and waits for the owner's approval before any code is written.
- Recommended runtime: the repository's own CI, through a `monochange/actions/agent` workflow (for example the Claude Code GitHub Action). monochange triggers it with an issue comment or `repository_dispatch`. Private code never leaves the repository's runners, the repository owns the model key and cost, and the same shape works on GitLab CI.
- Alternatives: GitHub's Copilot coding agent (GitHub-only, needs a Copilot plan), or agents hosted by monochange.dev (simplest UX, but monochange then holds code access and pays for compute).
- The agent works on `feat|fix/feedback-{id}`, opens a pull request that never merges itself, and its progress (plan, commits, review state) is mirrored into the feedback timeline and onto the user's original issue. The release that ships it notifies every subscriber.
- Guardrails already in the feedback crate: screened text never reaches an agent unreviewed, the brief treats reports as data, and only maintainers accept and merge.

### 5. GitLab, Gitea, and Forgejo

- **GitLab** has no app-installation model. Sign-in and organisation (group) access use an OAuth application with refresh tokens (gitlab.com and self-managed instances). Project and group access tokens, which create bot users, require Premium or Ultimate on gitlab.com, so on the free tier monochange acts through the signed-in user's token. Webhooks are registered per project through the API with a secret token. Commits created through the API can be signed by GitLab's instance key when web-commit signing is enabled, and only when the commit author matches the token's user. Merge requests and issues map directly onto the pipeline.
- **Gitea and Forgejo** (including Codeberg): OAuth2 applications per instance, organisation and repository APIs, repository webhooks, and multi-file commit APIs. Self-hosted instances need an "add your instance" flow.
- The existing `monochange_gitlab`, `monochange_gitea`, and `monochange_forgejo` crates already speak each provider's release-request API, so the app adapters reuse them.
- CI agents: GitLab CI and Gitea/Forgejo Actions equivalents of the phase 4 workflow.

## Decisions

| Decision      | Choice                                                                                                       |
| ------------- | ------------------------------------------------------------------------------------------------------------ |
| Sequencing    | GitHub end to end first (phases 1–4), then GitLab, Gitea, and Forgejo as adapters. Phase 2 runs alongside 1. |
| Agent runtime | The repository's own CI through a `monochange/actions/agent` workflow, triggered by monochange.              |
| Editor        | Markdown-backed rich editor island with toolbar, screenshot paste, and preview; sanitised on the server.     |
| Portal URL    | `/p/{org}/{project}` for now; custom domains later.                                                          |

## What happens to this branch

- Keep `monochange_app_feedback`: it is the phase 3 core and is independent of storage and provider.
- Drop the standalone `monochange_app_feedback_demo` server; its playground returns in phase 3 as a sandbox on monochange.dev, reusing the design.
