# GitHub App installation

<!-- {=hostedAppInstallation} -->

The monochange GitHub App connects the repositories you grant it access to. Hosted release commits and pull requests under its bot identity require a separate rollout. The CLI remains available without the app, and monochange is free.

## Availability

Sign in at [monochange.dev](https://monochange.dev/login) and open the dashboard to check repository connection availability. When the deployment has a configured GitHub App, **Connect repositories on GitHub** opens that app's installation page. If connection is unavailable, the dashboard says so; the CLI and local release planning remain available.

Website sign-in identifies your account. Installing the GitHub App grants access to the repositories you choose. A connected repository does not mean hosted release automation is enabled: hosted bot operations still require a separate verified rollout.

## Install the GitHub App

When the dashboard offers repository connection:

1. Sign in and choose **Connect repositories on GitHub** in the dashboard.
2. Select the GitHub account or organization that owns the repository. An organization owner may need to approve the installation.
3. Choose **All repositories** for that account, or **Only select repositories** for a smaller selection. Repeat installation for another account or organization when needed.
4. Review the requested access. Repository connection needs the required metadata read permission. Contents and pull-request write access are only needed for a later hosted release workflow.
5. Complete installation and return to the dashboard. Refresh the repository list after GitHub delivers the installation webhook.

Personal installations belong to the account owner. Organization installations appear for the user who installed the app while GitHub confirms they remain an organization owner; the dashboard does not automatically share them with every organization member.

You can change the selected repositories, suspend access, or uninstall the app in GitHub's installed-app settings. Installing the bot does not publish packages automatically.

## Connect the release workflow

After hosted bot operations have been enabled and verified, configure the release workflow's commit and pull-request steps to use the hosted backend:

```toml
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", backend = "hosted" },
]
```

GitHub Actions authenticates with its OIDC token. Grant `id-token: write` to the job that calls the hosted release steps. Other CI systems can use a monochange API token stored as `MONOCHANGE_TOKEN` in the CI secret store; never commit it to the repository.

Every run rebuilds the release pull request branch from `[source.pull_requests].base` and the bot replaces the previous release commit, so each push refreshes the open release pull request. A run whose base branch moved while it was preparing the release fails with a conflict and leaves the refresh to the run for the newer commit. The bot only replaces monochange release branches: the branch must be named `<branch_prefix>/release`, which `monochange step commit-release`, `monochange step open-release-request`, and a `[cli.release]` workflow produce, and it must not be the repository's default branch. Workflows with other names, such as `[cli.release-pr]`, get `<branch_prefix>/release-pr`; the first run creates that branch, but later runs fail with a conflict until the branch is deleted.

Keep your repository's branch protection and required checks enabled. Review and merge the bot's release pull request through your normal process; registry publishing remains a separate workflow.

<!-- {/hostedAppInstallation} -->

For the CLI-only provider workflow, see [GitHub automation](./08-github-automation.md). For registry credentials and publishing configuration, see [Trusted publishing and OIDC](./07-trusted-publishing.md).
