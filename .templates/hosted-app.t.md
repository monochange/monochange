<!-- {@hostedAppInstallation} -->

The monochange GitHub App adds a bot identity to the hosted release workflow. It creates release commits and release pull requests for the repositories you grant it access to. The CLI remains available without the app, and monochange is free.

## Availability

Check [the installation page](https://monochange.dev/install#github-app) for current hosted app availability. The public GitHub App installation is still being finalized; the CLI and local release planning are available now. GitHub sign-in on the website is separate from installing the bot on a repository.

## Install the GitHub App

Once installation is available:

1. Open the installation page and choose **Install on GitHub**.
2. Select the GitHub account or organization that owns the repository. An organization owner may need to approve the installation.
3. Choose **Only select repositories** and select the repositories monochange should manage.
4. Review the requested access. The hosted release workflow needs repository contents and pull-request write access; GitHub also grants the required metadata read access.
5. Complete installation, then follow the hosted workflow configuration below.

You can change the selected repositories, suspend access, or uninstall the app in GitHub's installed-app settings. Installing the bot does not publish packages automatically.

## Connect the release workflow

Configure the release workflow's commit and pull-request steps to use the hosted backend:

```toml
steps = [
	{ type = "PrepareRelease", name = "plan release", allow_empty_changesets = true },
	{ type = "CommitRelease", commit_backend = "hosted" },
	{ type = "OpenReleaseRequest", backend = "hosted" },
]
```

GitHub Actions authenticates with its OIDC token. Grant `id-token: write` to the job that calls the hosted release steps. Other CI systems can use a monochange API token stored as `MONOCHANGE_TOKEN` in the CI secret store; never commit it to the repository.

Keep your repository's branch protection and required checks enabled. Review and merge the bot's release pull request through your normal process; registry publishing remains a separate workflow.

<!-- {/hostedAppInstallation} -->
