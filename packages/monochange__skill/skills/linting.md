# Linting

`monochange check` validates monochange configuration, changesets, and package manifests using configured lint rules.

Use `monochange step validate` when you only need to know whether monochange can load the workspace and changesets. Use `monochange check` when package metadata consistency matters: publishability fields, workspace dependency protocols, duplicate changesets, package ownership, and other ecosystem-specific policy.

## Commands

```bash
monochange check
monochange check --fix
monochange lint list
monochange lint explain <rule-or-preset-id>
```

MCP equivalents:

- `monochange_lint_catalog`: list rules and presets for an agent UI or planning step.
- `monochange_lint_explain`: explain why a rule exists, which manifests it applies to, and what remediation usually looks like.

<!-- {=manifestRepositoryLintReadmeSummary} -->

### Repository Lints

monochange includes opt-in repository URL lint rules for Cargo, Dart, and npm-family manifests:

```toml
[lints.rules]
"cargo/manifest-repository" = "error"
"dart/manifest-repository" = "error"
"npm/manifest-repository" = "error"
```

These rules compare each manifest's `repository` field with the repository configured under `[source]` in `monochange.toml`. Root-level packages use the base repository URL, while packages in subdirectories use `{repo_url}/tree/{default_branch}/{relative_package_dir}`. Run `monochange check --fix` to insert or update repository fields; there is no per-rule `fix` option.

Cargo also resolves `repository = { workspace = true }` by reading the root manifest's `[workspace.package].repository` (falling back to root `[package].repository`). If you intentionally want to allow workspace inheritance without validating the package-specific URL, configure:

```toml
[lints.rules]
"cargo/manifest-repository" = { level = "error", allow_workspace_inheritance = true }
```

For full rule-by-rule behavior, see the manifest linting reference and `monochange lint explain <rule-id>`.

<!-- {/manifestRepositoryLintReadmeSummary} -->

## Configuration

```toml
[lints]
use = ["cargo/recommended", "npm/recommended"]
exclude = ["examples/**", "fixtures/**"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"
"changesets/duplicate" = "error"

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

Rules accept either a simple severity (`"error"`, `"warning"`, `"off"`) or a table with `level` and rule-specific options.

Use presets for the baseline policy and then layer explicit rules or scopes for exceptions. Scopes are useful when published packages need stricter metadata than fixtures, examples, private tools, or generated manifests.

Run `monochange check` before release previews and before merging configuration changes.
