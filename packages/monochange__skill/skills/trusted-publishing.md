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
