# Hosted workflow dispatch through the monochange app

**Status**: in progress **Owner**: release automation

## Problem

`monochange/monochange` releases need to refresh the separate `monochange/actions` release stream, and maintainers need to trigger the actions release-pr merge workflow without storing a personal access token in this repository. The interim `MONOCHANGE_ACTIONS_TOKEN` approach requires a second credential and duplicates the GitHub App installation that already exists for hosted release commits.

## Decision

Add a hosted workflow dispatch surface:

- `monochange gh workflow run <workflow> --repo <owner/repo> --ref <ref> [--field KEY=VALUE]` calls the monochange app.
- GitHub Actions callers authenticate with the job's OIDC token; local callers use `--hosted-auth token` with a deployment `MONOCHANGE_TOKEN`.
- The app authorizes OIDC callers against the GitHub App installation that owns both the calling repository and the target repository. It never allows cross-installation dispatch. API-token callers remain limited to `MONOCHANGE_TOKEN_REPOSITORIES`.
- The app starts the existing workflow through its installation token. It does not edit workflow files and does not request Workflows write access.
- `publish.yml` uses the new command for the `monochange/actions` handoff, removing `MONOCHANGE_ACTIONS_TOKEN`.

## Affected areas

- `crates/monochange_core/src/lib.rs` — hosted request and response types.
- `crates/monochange/src/workflow_dispatch.rs` — `monochange gh workflow run` command.
- `crates/monochange/src/cli.rs` and `crates/monochange/src/lib.rs` — command registration and dispatch.
- `crates/monochange/src/release_artifacts.rs` — shared hosted auth for the new operation.
- `app/crates/monochange_app_api/src/workflow_api.rs` — `POST /api/workflow-dispatches`.
- `app/crates/monochange_app_api/src/github_app.rs` — installation-token workflow dispatch.
- `.github/workflows/publish.yml` — actions release handoff.
- `.templates/hosted-app.t.md` — shared app documentation.
- `docs/agents/downstream-rollout.md` and `docs/agents/release-checklist.md`.

## Acceptance checks

- [ ] `monochange gh workflow run` parses the workflow, repository, ref, fields, output format, and hosted auth options.
- [ ] The app rejects malformed repositories, workflow names, refs, and inputs before calling GitHub.
- [ ] OIDC callers can dispatch within one installation and cannot dispatch across installations.
- [ ] API-token callers can only target repositories in `MONOCHANGE_TOKEN_REPOSITORIES`.
- [ ] A successful dispatch reaches GitHub with the expected `ref` and `inputs`.
- [ ] Dry runs validate authorization without calling GitHub.
- [ ] The monochange CLI release dispatches the actions release workflow through the app and stores no personal access token.
- [ ] `monochange step validate`, affected-package verification, Rust tests, app tests, `zizmor`, and `dprint` pass.
