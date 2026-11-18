---
monochange: feat
monochange_core: feat
monochange_app: feat
---

# Dispatch GitHub Actions workflows through the monochange app

`monochange gh workflow run` dispatches a workflow in a connected repository without a personal access token. The command mirrors the useful part of `gh workflow run`:

```bash
monochange gh workflow run release.yml --repo monochange/actions --ref main
monochange gh workflow run release-pr-merge.yml --repo monochange/actions -f pull_request=103
```

A GitHub Actions job authenticates with its OIDC token by granting `id-token: write`; local runs use `--hosted-auth token` and a deployment `MONOCHANGE_TOKEN`. The monochange app checks that an OIDC caller and the target repository belong to the same GitHub App installation, so an organisation repository can dispatch a sibling repository's release workflow while other installations remain isolated. `--dry-run` validates the target without starting a run, and `--field KEY=VALUE` passes workflow inputs.

The new `HostedWorkflowDispatchRequest` and `HostedWorkflowDispatchResponse` types expose the same contract to other clients. `publish.yml` now uses the command to refresh the `monochange/actions` release PR after a CLI release, replacing the `MONOCHANGE_ACTIONS_TOKEN` credential.
