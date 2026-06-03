---
"monochange": major
---

# Move user-defined CLI commands under `monochange run`

Repository-defined commands from `[cli.<name>]` tables in `monochange.toml` now execute through the `monochange run <name>` namespace instead of occupying top-level CLI command names.

Before:

```toml
[cli.release]
description = "Prepare a release"
steps = [
	{ type = "PrepareRelease", dry_run = true },
]
```

```sh
monochange release --dry-run
monochange publish --registry npm
monochange validate-workspace
```

After:

```toml
[cli.release]
description = "Prepare a release"
steps = [
	{ type = "PrepareRelease", dry_run = true },
]
```

```sh
monochange run release --dry-run
monochange run publish --registry npm
monochange run validate-workspace
```

The `monochange.toml` command definitions do not need to move or change shape. The invocation path is the breaking change.

## Rationale

Top-level CLI names are now reserved for built-in monochange commands such as `check`, `change`, `release`, `versions`, `step`, and `run`. Moving repository-defined commands under `run` prevents a project-local command from shadowing a built-in command added in a future release, and it makes automation easier to read: `monochange run <name>` always means “execute a command from this repository's config.”

## Migration guidance

Update every script, CI workflow, release workflow, agent skill, and README command that invokes a `[cli.<name>]` command directly:

- `monochange release-pr` becomes `monochange run release-pr` when `release-pr` is defined in `monochange.toml`.
- `monochange publish --dry-run` becomes `monochange run publish --dry-run` when `publish` is defined in `monochange.toml`.
- `monochange validate-workspace` becomes `monochange run validate-workspace` when `validate-workspace` is defined in `monochange.toml`.

Do not add `run` for built-in commands. For example, keep `monochange check`, `monochange versions --format json`, and `monochange step validate` as built-in command invocations.
