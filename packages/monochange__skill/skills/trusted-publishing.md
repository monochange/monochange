# Trusted publishing

monochange publishing settings can opt packages into trusted/OIDC publishing where supported.

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[package."@acme/api".publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true
```

Use a table when the trust context needs explicit repository/workflow/environment metadata:

```toml
[package."@acme/api".publish.trusted_publishing]
enabled = true
repository = "acme/widgets"
workflow = "publish.yml"
environment = "npm"
```

Registry support and setup requirements vary. Treat trusted-publishing setup as a registry-side operation that may require a human maintainer. Agents should generate configuration and workflow code, but should not use local credentials or perform registry-side changes unless explicitly authorized and allowed by project policy.

## pub.dev requires a tag run ref

pub.dev validates the run ref inside the GitHub OIDC token for every publish, including `workflow_dispatch` runs: the ref must be `refs/tags/<tag-pattern>`, where `<tag-pattern>` is configured in the package's pub.dev admin and contains `{{version}}` matching the published version. The "Enable publishing from `workflow_dispatch` events" checkbox on pub.dev only allows the event name; a workflow dispatched on a branch (`refs/heads/*`) is always rejected with "publishing is only allowed from 'tag' refType".

To publish through `workflow_dispatch`, push the release tag first and dispatch the workflow on that tag so the run's ref is the tag:

```sh
git push origin refs/tags/<package>-v<version>
gh workflow run publish.yml --ref <package>-v<version>
```

A single run ref authorizes only packages whose pub.dev tag pattern and version match it, so in a monorepo each dart package version needs its own tagged dispatch (or a shared pattern when versions move together). monochange fails the publish before minting any token when the run ref is not a tag; a non-OIDC `PUB_TOKEN` credential bypasses the policy.
