# Quickstart: npm packages

```bash
monochange init
monochange step validate
monochange step discover --format json
```

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
lockfile_commands = ["pnpm install --lockfile-only"]
```

Create release intent, then preview:

```bash
monochange step create-change-file --package @acme/api --bump minor --reason "Add webhook filters"
monochange step validate
monochange step prepare-release --dry-run --format json
```
