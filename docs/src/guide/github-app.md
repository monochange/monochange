# GitHub App installation

<!-- {=hostedAppInstallation} -->

The monochange GitHub App connects the repositories you grant it access to and gives your release workflow a bot identity. With the app installed, a GitHub Actions job can create the release commit and the release pull request as the monochange bot without a personal access token or any repository secret. The CLI remains available without the app, and monochange is free.

## Why use the app for release pull requests

A release workflow has two ways to push the release branch without the app, and both have a cost:

- With `GITHUB_TOKEN`, GitHub does not start workflows for the pushed branch or the opened pull request, so the release pull request never runs its own checks.
- With a personal access token, checks run, but the commits belong to a person, the token expires, and every repository needs its own copy of the secret.

The app removes both costs. The workflow proves which repository it runs for with a GitHub Actions OIDC token, and monochange.dev creates the commit and the pull request through the app installation. The commit is verified by GitHub, the pull request triggers your workflows, and one installation covers every repository you select.

## Availability

Sign in at [monochange.dev](https://monochange.dev/login) and open the dashboard to check repository connection availability. When the deployment has a configured GitHub App, **Connect repositories on GitHub** opens that app's installation page. If connection is unavailable, the dashboard says so; the CLI and local release planning remain available.

Website sign-in identifies your account. Installing the GitHub App grants access to the repositories you choose. A connected repository changes nothing until a release workflow opts into the hosted backend.

## Install the GitHub App

When the dashboard offers repository connection:

1. Sign in and choose **Connect repositories on GitHub** in the dashboard.
2. Select the GitHub account or organization that owns the repository. An organization owner may need to approve the installation.
3. Choose **All repositories** for that account, or **Only select repositories** for a smaller selection. Repeat installation for another account or organization when needed.
4. Review the requested access. Metadata read access connects the repository. Contents and pull-request write access let the bot create release commits and release pull requests. The app does not request Actions, Administration, or Workflows access.
5. Complete installation and return to the dashboard. Refresh the repository list after GitHub delivers the installation webhook. A repository that is not listed cannot use the hosted backend yet.

Personal installations belong to the account owner. Organization installations appear for the user who installed the app while GitHub confirms they remain an organization owner; the dashboard does not automatically share them with every organization member.

You can change the selected repositories, suspend access, or uninstall the app in GitHub's installed-app settings. Installing the bot does not publish packages automatically.

## Connect the release workflow

Configure the release workflow's commit and pull-request steps to use the hosted backend:

```toml
[cli.release]
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", name = "commit release", inputs = { stage_all = true, commit_backend = "hosted" } },
	{ type = "OpenReleaseRequest", name = "open release request", inputs = { backend = "hosted" } },
]
```

When `CommitRelease` sets `stage_all = true`, the hosted commit also carries every other changed, added, or deleted file in the workspace, such as lockfiles or generated files that earlier `Command` steps rewrote, matching the local backend's `git add --all`. Files that `.gitignore` excludes are left out.

Then grant `id-token: write` to the GitHub Actions job that runs the workflow. The job no longer needs `contents: write`, a personal access token, `gh auth setup-git`, or a `git config user.name` step:

```yaml
jobs:
  release-pr:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
      id-token: write
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
          persist-credentials: false
      - run: npm install --global @monochange/cli
      - run: monochange run release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

`GH_TOKEN` is only used for read-only lookups, such as linking changelog entries to their pull requests. The hosted backend authenticates with the job's OIDC token, so a job without `id-token: write` fails with `hosted CommitRelease token auth requires MONOCHANGE_TOKEN`. The `MONOCHANGE_TOKEN` fallback is for deployments that configure an API token; monochange.dev does not issue per-user tokens, so use GitHub Actions for hosted release commits.

The release branch only exists on GitHub after a hosted run. A later step that reads it, such as `monochange step release-record --from monochange/release/release`, must fetch it first with `git fetch origin "+refs/heads/monochange/release/release:refs/heads/monochange/release/release"`.

If pull-request preflight jobs or local runs use the same `[cli.release]` command, declare a `backend` choice input that defaults to `local`, pass `commit_backend = "{{ inputs.backend }}"` and `backend = "{{ inputs.backend }}"` to the two steps, and run `monochange run release --backend hosted` only from the release job.

Every run rebuilds the release pull request branch from `[source.pull_requests].base` and the bot replaces the previous release commit, so each push refreshes the open release pull request. A run whose base branch moved while it was preparing the release fails with a conflict and leaves the refresh to the run for the newer commit. The bot only replaces monochange release branches: the branch must be named `<branch_prefix>/release`, which `monochange step commit-release`, `monochange step open-release-request`, and a `[cli.release]` workflow produce, and it must not be the repository's default branch. Workflows with other names, such as `[cli.release-pr]`, get `<branch_prefix>/release-pr`; the first run creates that branch, but later runs fail with a conflict until the branch is deleted. The bot never writes files under `.git` or `.github/workflows`.

Keep your repository's branch protection and required checks enabled. Review and merge the bot's release pull request through your normal process; registry publishing remains a separate workflow.

<!-- {/hostedAppInstallation} -->

For the CLI-only provider workflow, see [GitHub automation](./08-github-automation.md). For registry credentials and publishing configuration, see [Trusted publishing and OIDC](./07-trusted-publishing.md).
