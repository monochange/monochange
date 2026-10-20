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
4. Review the requested access. Metadata read access connects the repository. Contents, pull-request, and issue write access let the bot create release commits, release pull requests, and issues for accepted feedback. Actions and checks access let monochange follow the builds agents run in your CI, re-run or cancel them, and report its own checks, and commit-status read access covers CI that reports statuses. Organization members read access confirms that you still own an organization installation. The app does not request Administration, Secrets, or Workflows access, so it can't change your repository settings or workflow files.
5. Complete installation and return to the dashboard. Refresh the repository list after GitHub delivers the installation webhook. A repository that is not listed cannot use the hosted backend yet.

Personal installations belong to the account owner. Organization installations appear for the user who installed the app while GitHub confirms they remain an organization owner; the dashboard does not automatically share them with every organization member.

You can change the selected repositories, suspend access, or uninstall the app in GitHub's installed-app settings. Installing the bot does not publish packages automatically.

## When the app asks for more access

When monochange starts requesting a new permission, GitHub emails the owner of each account where the app is installed and keeps that installation on the access it already approved until an owner accepts the request. The dashboard lists every connected account that is still waiting, with the permissions it is missing and a **Review on GitHub** link to the installation's settings, where an owner accepts the request. Signing in to monochange again is not needed: the bot's access comes from the installation, and your sign-in follows it once the request is approved.

## Connect the release workflow

Run the release workflow's commit and pull-request steps on the hosted backend. The simplest way is to set `MONOCHANGE_RELEASE_BACKEND=hosted` on the release job, which moves every `CommitRelease` and `OpenReleaseRequest` step to the hosted backend without editing `monochange.toml`. Local runs and pull-request preflight jobs keep the configured backend:

```toml
[cli.release]
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", name = "commit release", inputs = { stage_all = true } },
	{ type = "OpenReleaseRequest", name = "open release request" },
]
```

When `CommitRelease` sets `stage_all = true`, the hosted commit also carries every other changed, added, or deleted file in the workspace, such as lockfiles or generated files that earlier `Command` steps rewrote, matching the local backend's `git add --all`. Files that `.gitignore` excludes are left out.

Then give the job `id-token: write`, `contents: write`, and the workflow's `GITHUB_TOKEN`:

```yaml
jobs:
  release-pr:
    runs-on: ubuntu-latest
    permissions:
      contents: write
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
          MONOCHANGE_RELEASE_BACKEND: hosted
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

The job no longer needs a personal access token, `gh auth setup-git`, or a `git config user.name` step. The hosted backend authenticates with the job's OIDC token, so a job without `id-token: write` fails with `hosted CommitRelease token auth requires MONOCHANGE_TOKEN`. The `MONOCHANGE_TOKEN` fallback is for deployments that configure an API token; monochange.dev does not issue per-user tokens, so use GitHub Actions for hosted release commits.

### How the release commit is built

The CLI uploads the release files to the repository as git blobs and a tree with `GITHUB_TOKEN` (or `GH_TOKEN`), then sends monochange.dev only the tree id, the base commit, and the commit message. monochange.dev creates the commit on that tree through the app installation and moves the release branch, so the commit is verified and belongs to the bot. `contents: write` is what lets `GITHUB_TOKEN` create blobs and trees. Blobs and trees are not commits: uploading them pushes no branch and starts no workflow, and nothing in the repository refers to them until the bot commits. The token is also used for read-only lookups, such as linking changelog entries to their pull requests.

Without a token, or when the token lacks `contents: write`, the CLI prints a warning and sends the file contents to monochange.dev instead, which creates the blobs and the tree itself. That fallback only accepts UTF-8 text files and requests up to 8 MiB, so grant `contents: write` for binary files or very large changelogs.

To keep the backend in `monochange.toml` instead of the environment, set `commit_backend = "hosted"` on `CommitRelease` and `backend = "hosted"` on `OpenReleaseRequest`. A step input or command-line flag, such as `--commit-backend local`, still wins over `MONOCHANGE_RELEASE_BACKEND`.

The release branch only exists on GitHub after a hosted run. A later step that reads it, such as `monochange step release-record --from monochange/release/release`, must fetch it first with `git fetch origin "+refs/heads/monochange/release/release:refs/heads/monochange/release/release"`.

Every run rebuilds the release pull request branch from `[source.pull_requests].base` and the bot replaces the previous release commit, so each push refreshes the open release pull request. A run whose base branch moved while it was preparing the release fails with a conflict and leaves the refresh to the run for the newer commit. The bot only replaces monochange release branches: the branch must be named `<branch_prefix>/release`, which `monochange step commit-release`, `monochange step open-release-request`, and a `[cli.release]` workflow produce, and it must not be the repository's default branch. Workflows with other names, such as `[cli.release-pr]`, get `<branch_prefix>/release-pr`; the first run creates that branch, but later runs fail with a conflict until the branch is deleted. The bot never writes files under `.git` or `.github/workflows`: monochange.dev refuses uploaded paths there and refuses a tree whose `.github/workflows` differs from the base commit's.

Keep your repository's branch protection and required checks enabled. Review and merge the bot's release pull request through your normal process; registry publishing remains a separate workflow.

<!-- {/hostedAppInstallation} -->

For the CLI-only provider workflow, see [GitHub automation](./08-github-automation.md). For registry credentials and publishing configuration, see [Trusted publishing and OIDC](./07-trusted-publishing.md).
