---
monochange: minor
monochange_core: major
monochange_schema: patch
---

# Hosted release commits through the monochange GitHub App

`CommitRelease` and `OpenReleaseRequest` can now delegate their writes to the monochange app so the monochange GitHub App creates the release commit and the release pull request. A release PR built this way is committed by the bot identity, is verified by GitHub, triggers the repository's normal workflows, and satisfies verified-commit branch protection — without a personal access token.

Set `commit_backend = "hosted"` on `CommitRelease` and `backend = "hosted"` on `OpenReleaseRequest`:

```toml
[cli.release]
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", name = "create release commit", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", name = "create the pr", backend = "hosted" },
]
```

The local `git commit` path remains the default and is unchanged. The release is still computed in the workflow: monochange reads the prepared release files, resolves the target branch and the base commit, and posts a compact request to the app's `/api/release-commits` and `/api/release-requests` endpoints. The server verifies GitHub's OIDC token for the run (or the `MONOCHANGE_TOKEN` secret on CI systems without OIDC), mints a one-hour installation token, creates blobs/tree/commit through the Git Database API, guards against a branch that moved since preparation, and opens or updates the pull request under the bot identity.

New step inputs:

- `commit_backend` (`local` | `hosted`) — how the release commit is written.
- `hosted_auth` (`auto` | `oidc` | `token`) — how the hosted backend authenticates; `auto` prefers the GitHub Actions OIDC token and falls back to `MONOCHANGE_TOKEN`.
- `hosted_url` — the app base URL (default `https://monochange.dev`).
- `oidc_audience` — the audience required in the OIDC token (default: the hosted URL host).
- `backend` (`local` | `hosted`) on `OpenReleaseRequest` — how the pull request is published.

Workflows using the hosted backend need `permissions: id-token: write` for OIDC, or the `MONOCHANGE_TOKEN` secret as a fallback. A rerun whose branch moved fails with a conflict instead of overwriting newer work, and consumed changeset deletions travel in the same request as file updates.

## Breaking change in `monochange_core`

`CliStepDefinition::CommitRelease` gained the fields `commit_backend`, `hosted_auth`, `hosted_url`, and `oidc_audience`; `CliStepDefinition::OpenReleaseRequest` gained `backend`, `hosted_auth`, and `hosted_url`. Every exhaustive struct literal for these variants must add the new fields — use `..Default::default()`-style struct update or the listed defaults:

```rust
// before
CliStepDefinition::CommitRelease { name: None, when: None, always_run: false, no_verify: false, update_release_json: false, stage_all: false, inputs: BTreeMap::new() }
// after
CliStepDefinition::CommitRelease { name: None, when: None, always_run: false, no_verify: false, update_release_json: false, stage_all: false, commit_backend: Default::default(), hosted_auth: Default::default(), hosted_url: None, oidc_audience: None, inputs: BTreeMap::new() }
```

Deserialization is unaffected: every new field has a serde default, so existing `monochange.toml` step tables keep parsing identically and the local backends remain the default behaviour.
