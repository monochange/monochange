---
"monochange": fix
---

# Quote scoped package ids in the config `monochange init` generates

`monochange init` in a repository with scoped npm packages wrote the package table header unquoted:

```toml
[package.@acme/sdk]      # generated before — not a legal TOML key
path = "packages/sdk"
type = "npm"
```

TOML bare keys may only contain letters, digits, `-`, and `_`, so the config the command had just written failed to parse on the next run:

```text
error[config.invalid]: TOML parse error ... invalid unquoted key, expected letters, numbers, `-`, `_`
```

This affected every monorepo using scoped npm package names, including this repository's own npm fixtures, and `init` is the first command a new user runs.

Package ids are now rendered through a TOML key escaper: a legal bare key stays bare (`[package.acme-cli]`) and anything else is emitted quoted (`[package."@acme/sdk"]`). The group member list already quoted correctly, and the init-generated config now passes `monochange step validate` for mixed cargo and scoped-npm repositories.
