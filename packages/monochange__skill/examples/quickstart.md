# Quickstart: npm packages

Scaffold and inspect the workspace:

```bash
monochange init
monochange step validate
monochange step discover --format json
```

A minimal config for two npm packages. Every `lockfile_commands` entry is a table with a `command` field, because monochange rejects a bare string:

```toml
[defaults]
package_type = "npm"
parent_bump = "patch"

[package."@acme/api"]
path = "packages/api"

[package."@acme/ui"]
path = "packages/ui"

[ecosystems.npm]
enabled = true
lockfile_commands = [
	# `cwd` is relative to the workspace root and defaults to the root.
	{ command = "pnpm install --lockfile-only" },
]
```

Create release intent, then preview the plan:

```bash
monochange step create-change-file --package @acme/api --bump minor --reason "Add webhook filters"
monochange step validate
monochange step prepare-release --dry-run --format json
```

The preview prints every planned version, changelog entry, and file write without touching the tree.
