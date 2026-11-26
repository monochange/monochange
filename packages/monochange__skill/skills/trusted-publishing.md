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
mode = "preferred"
repository = "acme/widgets"
workflow = "publish.yml"
environment = "npm"
```

`publish.trusted_publishing.mode` supports both publishing paths from one configuration:

- `mode = "required"` (default) fails any local or manual publish; trusted publishing is the only allowed path.
- `mode = "preferred"` uses trusted publishing when a verifiable CI identity is detected and falls back to local registry credentials otherwise, so `monochange run publish` works from a maintainer machine while CI still verifies the repository, workflow, and environment.

Registry support and setup requirements vary. Treat trusted-publishing setup as a registry-side operation that may require a human maintainer. Agents should generate configuration and workflow code, but should not use local credentials or perform registry-side changes unless explicitly authorized and allowed by project policy.

## pub.dev requires a tag run ref

pub.dev validates the run ref inside the GitHub OIDC token for every publish, including `workflow_dispatch` runs: the ref must be `refs/tags/<tag-pattern>`, where `<tag-pattern>` is configured in the package's pub.dev admin and contains `{{version}}` matching the published version. The "Enable publishing from `workflow_dispatch` events" checkbox on pub.dev only allows the event name; a workflow dispatched on a branch (`refs/heads/*`) is always rejected with "publishing is only allowed from 'tag' refType".

To publish through `workflow_dispatch`, push the release tag first and dispatch the workflow on that tag so the run's ref is the tag:

```sh
git push origin refs/tags/<package>-v<version>
gh workflow run publish.yml --ref <package>-v<version>
```

A single run ref authorizes only packages whose pub.dev tag pattern and version match it, so in a monorepo each dart package version needs its own tagged dispatch (or a shared pattern when versions move together). `monochange publish packages` applies the rule before minting any token. A trusted pub.dev package that the run cannot upload (another ref, a branch, an event other than `push` or `workflow_dispatch`, or no `GITHUB_REF`) is marked `deferred_to_tag`, and `deferred_to` names the `reason`, the tag, and the `gh workflow run` command that publishes it. When a release has a trusted pub.dev package, monochange gives each package of the release to exactly one run on the release's tags: a trusted pub.dev package to the run on its own tag, and every other package to the run on the primary tag (without one, to its own tag or the first tag no trusted pub.dev package uses). Runs on the release's other tags mark the package `skipped_out_of_scope` and name the run that publishes it, so the publish workflow must start one run for every tag the release creates. Runs on branches and on tags the release does not create publish everything but the trusted pub.dev packages. A `wrong_event` or `unknown_ref` deferral ends the run with a warning, because no run of that workflow can publish the package until the workflow is fixed. A non-empty `PUB_TOKEN` bypasses the policy; set `publish.trusted_publishing = false` for such a package so `monochange step validate` stops requiring a release tag.
