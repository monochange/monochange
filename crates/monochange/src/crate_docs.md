# `monochange`

<!-- {=monochangeCrateDocs} -->

`monochange` is the top-level entry point for the workspace.

Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, Dart/Flutter, Python, and Go workspaces, exposes top-level commands from `monochange.toml`, and runs configured CLI commands from those definitions.

## Why use it?

- coordinate one config-defined CLI across several package ecosystems
- expose discovery, change creation, and release preparation as both commands and library calls
- connect configuration loading, package discovery, graph propagation, and semver evidence in one place

## Best for

- shipping the `monochange` CLI in CI or local release tooling
- embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
- generating starter config with `monochange init` and then evolving the CLI command surface over time

## Key commands

```bash
monochange init
monochange skill -a pi -y
monochange step discover --format json
monochange run change --package monochange --bump patch --reason "describe the change"
monochange step prepare-release --dry-run --format json
monochange mcp
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- load config-defined `[cli.*]` workflow commands from `monochange.toml`
- expose binary commands such as `init`, `check`, `analyze`, `mcp`, `help`, and `version`
- generate immutable `monochange step *` commands from the built-in step schemas
- resolve change input files
- render discovery and release command output in text or JSON
- execute configured workflow commands plus built-in MCP commands
- preview or publish provider releases from prepared release data
- evaluate pull-request changeset policy from CI-supplied changed paths and labels
- expose JSON-first MCP tools for assistant workflows

<!-- {/monochangeCrateDocs} -->
