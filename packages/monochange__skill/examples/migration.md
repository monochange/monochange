# Migration: mixed Cargo and npm repository

A repository with one Rust crate and one npm package that release together. The group gives both packages a single outward version and tag, while each package keeps its own manifest handling.

```toml
[defaults]
# Severity added to a dependent package when this package changes.
parent_bump = "patch"

[package.acme_core]
path = "crates/acme_core"
type = "cargo"
# Stamp a second Cargo manifest beyond the package's own.
versioned_files = ["Cargo.toml"]

[package."@acme/cli"]
path = "packages/cli"
type = "npm"

[group.main]
packages = ["acme_core", "@acme/cli"]
# The group owns the top-level v1.2.3 tag and the provider release.
tag = true
release = true
version_format = "primary"
changelog = { path = "CHANGELOG.md", format = "keep_a_changelog" }

[ecosystems.cargo]
enabled = true
lockfile_commands = [
	{ command = "cargo generate-lockfile" },
]

[ecosystems.npm]
enabled = true
lockfile_commands = [
	# Run in the package directory rather than the workspace root.
	{ command = "pnpm install --lockfile-only", cwd = "packages/cli" },
]
```

Validate the result:

```bash
monochange step validate
monochange check
monochange step discover --format json
```

`step validate` checks package, group, and `versioned_files` rules. `check` adds the manifest lint rules configured under `[lints]`.
