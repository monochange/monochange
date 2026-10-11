---
monochange: fix
monochange_hosting: fix
---

# Commit hosted releases to the release pull request branch

Workflows that call the hosted backend from a push to `main` sent the release commit to the branch that triggered the run (`GITHUB_HEAD_REF`, then `GITHUB_REF_NAME`), so the monochange app was asked to commit onto `main` instead of the release pull request branch. Hosted `CommitRelease` now always targets the branch that `OpenReleaseRequest` opens, derived from `[source.pull_requests]` `branch_prefix`, and reports the configured `base` branch so the app can tell a stale run from a refresh.

Built-in step commands also derived the release branch from the step name, so `monochange step open-release-request` opened `monochange/release/step-open-release-request`. `release_pull_request_branch` in `monochange_hosting` now maps every `step <name>` command to the default `release` branch for all providers:

```bash
monochange step commit-release --commit-backend hosted
monochange step open-release-request --backend hosted
# before: commit to `main`, pull request from `monochange/release/step-open-release-request`
# after:  both use `monochange/release/release`
```

The hosted request also sent the release record by its absolute runner path (for example `/home/runner/work/repo/repo/.monochange/releases/<id>/release.json`), which the app rejects as not repository-relative. Release records are now sent as `.monochange/releases/<id>/release.json`.

Workflows that set `GITHUB_HEAD_REF` to the release branch to work around this can drop that override.
