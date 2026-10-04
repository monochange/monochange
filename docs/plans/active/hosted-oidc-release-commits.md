# Hosted release commits and release requests

**Status**: In progress **Created**: 2026-05-14 (updated 2026-10-04) **Owner**: monochange app / release automation

---

## Problem statement

monochange release PR workflows need to update a release branch with generated release files, then run normal CI on the release PR before it is merged and published.

The current choices are not good enough for a generic user-facing product:

- `GITHUB_TOKEN` is safe and repository-scoped, but branch updates made with it do not trigger the follow-up workflows needed to validate the generated release PR.
- A personal access token can trigger workflows, but it is user-specific, hard to productize, and creates unbranded/unverified commits unless every user solves commit identity and signing separately.
- Local `git commit && git push` from CI is difficult to make work with repositories that require verified commits.

Release PR commits should be:

1. Created by the monochange GitHub App (bot) identity.
2. Verified by GitHub.
3. Able to trigger normal workflows.
4. Available to any repository that installs the monochange GitHub App.
5. Usable with a single `MONOCHANGE_TOKEN` secret (or no secret at all when GitHub Actions OIDC is available).

## Architecture

The CLI still computes the release locally — `PrepareRelease`, lockfile refreshes, and changelog rendering are unchanged. Only the write operations are delegated:

```text
GitHub Actions job
  ├─ checkout repo
  ├─ monochange run release --commit --create-pr
  │    ├─ PrepareRelease (local, unchanged)
  │    ├─ CommitRelease(commit_backend = "hosted")
  │    │    ├─ read tracked release files locally
  │    │    ├─ request GitHub Actions OIDC token (audience monochange.dev)
  │    │    └─ POST /api/release-commits
  │    └─ OpenReleaseRequest(backend = "hosted")
  │         └─ POST /api/release-requests

monochange app (monochange.dev)
  ├─ verify OIDC token (or MONOCHANGE_TOKEN)
  ├─ map repository claim → connected installation
  ├─ mint GitHub App installation token
  ├─ blobs → tree → commit via Git Database API
  │    (parent = prepared base commit; branch guard rejects moved branches)
  ├─ update the release branch ref
  └─ create or update the release pull request under the bot identity
```

### Why the pull request is also hosted

A `GITHUB_TOKEN`-created pull request would not trigger `pull_request` workflows, and after a hosted commit the local checkout no longer matches the remote branch. Hosting `OpenReleaseRequest` keeps the whole release PR — commit, branch, pull request, and labels — under one bot identity.

## Pieces

### CLI (this repository, released with the CLI)

- `monochange_core`:
  - `ReleaseBackend` (`local` default, `hosted`) on `CommitRelease` (`commit_backend`) and `OpenReleaseRequest` (`backend`).
  - `HostedCommitAuth` (`auto`, `oidc`, `token`) on `CommitRelease`.
  - `HostedCommitRequest` / `HostedCommitResponse` and `HostedReleaseRequest` / `HostedReleaseResponse` wire contracts.
- `monochange`:
  - `hosted_commit_release` builds the request from prepared release files (file content is read locally; a missing file deletes the path, which is how consumed changesets are removed), resolves the OIDC audience from the hosted URL host by default, and posts to `/api/release-commits`.
  - `hosted_release_request_result` posts the locally-rendered `SourceChangeRequest` to `/api/release-requests`.
  - Idempotency key `owner/repo:run-id:attempt:CommitRelease` when run inside GitHub Actions.

### App (`app/`, deployable to monochange.dev)

- `monochange_app_api::github_app` — GitHub App JWT (RS256, 9 min) and installation token minting; Git Database API commit creation with branch-moved guard; pull request create/update/label/auto-merge.
- `monochange_app_api::oidc` — GitHub Actions OIDC verification against `https://token.actions.githubusercontent.com/.well-known/jwks` with a 10-minute JWKS cache, issuer/audience validation, and repository claim extraction.
- `monochange_app_api::webhooks` — `POST /api/github/webhooks` receiver with constant-time HMAC-SHA256 signature verification; installation created/added/removed/deleted/suspend/unsuspended events keep the `installations` and `repositories` tables in sync.
- `monochange_app_api::release_api` — `POST /api/release-commits` and `POST /api/release-requests`: authenticate (OIDC or constant-time `MONOCHANGE_TOKEN`), require a connected installation, enforce the release-path allowlist (no `.git`, no `..`, no `.github/workflows` mutation), then delegate to `github_app`.
- `monochange_app` — website: landing, sign-in, dashboard (connected repositories with installation status), and `/install` setup page.

### Configuration

```toml
[cli.release]
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", name = "create release commit", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", name = "create the pr", backend = "hosted" },
]
```

Optional step inputs: `hosted_auth` (`auto`/`oidc`/`token`), `hosted_url` (default `https://monochange.dev`), `oidc_audience` (default: the hosted URL host).

Workflow requirements: `permissions: id-token: write` for OIDC (preferred), or the `MONOCHANGE_TOKEN` repository secret as fallback. No PAT, no deploy key, no git identity configuration.

## Security rules

- App credentials (`GITHUB_APP_ID`, private key, webhook secret) live only in the monochange deployment; users never see them.
- Installation tokens are minted per request and never returned to callers.
- Hosted commit paths must be repository-relative and may not touch `.git` or `.github/workflows`.
- The branch-moved guard rejects a rerun whose prepared base commit no longer matches the branch head (`409`).
- Webhook signatures are verified in constant time before any database write.
- API tokens are compared in constant time.

## What remains before dogfooding

- [ ] Create the production monochange GitHub App (human task).
- [ ] Deploy the app with production secrets (human task; see `app/DEPLOY.md`).
- [ ] Switch this repository's release workflow to the hosted backend and verify the generated commit is verified and triggers CI.
- [ ] Delete the `RELEASE_PR_MERGE_TOKEN` PAT once verified.

## Validation

- CLI: `cargo test -p monochange` (hosted request builder, options resolution, OIDC audience derivation, backend parsing).
- App: `cargo test --workspace` inside `app/` (webhook signatures, OIDC verification against a mock JWKS, token flows).
- Schemas: `cargo xtask schema release update --versioned`.
