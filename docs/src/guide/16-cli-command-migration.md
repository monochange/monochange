# Migrating CLI automation to the nested command API

This guide is for maintainers and agents updating repositories from the older monochange CLI command layout to the current nested command API.

The migration has three breaking command-path changes:

1. Built-in step commands moved to the nested `monochange step <name>` path; colon-delimited top-level aliases are no longer supported.
2. User-defined `[cli.<name>]` commands moved from `monochange <name>` to `monochange run <name>`.
3. The packaged `mc` binary alias was removed; use `monochange` directly.

## Quick replacement table

| Old command                         | New command                           |
| ----------------------------------- | ------------------------------------- |
| `mc check`                          | `monochange check`                    |
| `mc versions --format json`         | `monochange versions --format json`   |
| Colon-delimited built-in step token | `monochange step <name>`              |
| `monochange <configured-command>`   | `monochange run <configured-command>` |

## 1. Replace the `mc` binary alias

The release now ships only the `monochange` executable. Replace every `mc` invocation in scripts, CI workflows, documentation, and agent instructions.

Before:

```sh
mc check
mc versions --format json
```

After:

```sh
monochange check
monochange versions --format json
monochange step validate
```

If a local developer wants shorthand, they can define their own shell alias, but repository automation should not depend on it:

```nu
alias mc = monochange
```

## 2. Move built-in step commands under `step`

Built-in workflow steps no longer use colon-delimited top-level command names. Invoke them through the nested command path:

```sh
monochange step config --format json
monochange step validate
monochange step publish-readiness --format json
monochange step publish-packages --dry-run
```

The step flags and output formats stay attached to the step itself. Split the command path into `step` and `<name>` arguments; argument arrays should likewise use two entries.

## 3. Move configured commands under `run`

Commands defined in `monochange.toml` stay in `[cli.<name>]`, but they are invoked through `monochange run <name>`.

Given this config:

```toml
[cli.release-pr]
description = "Prepare a release pull request"
steps = [
	{ type = "PrepareRelease", dry_run = true },
	{ type = "OpenReleaseRequest", dry_run = true },
]
```

Before:

```sh
monochange run release-pr --dry-run
```

After:

```sh
monochange run release-pr --dry-run
```

Only add `run` for commands that come from `[cli.<name>]`. Built-in commands remain top-level or nested built-ins:

```sh
monochange check
monochange run change
monochange versions --format json
monochange step validate
```

## Agent checklist

When updating a repository, scan these files first:

- `.github/workflows/*.yml`
- `.gitlab-ci.yml`
- `devenv.nix` and task files
- `package.json` scripts
- `Cargo.toml` aliases or xtask wrappers
- `README.md` and docs examples
- `AGENTS.md`, skill files, prompt templates, and other agent instructions
- Shell scripts under `scripts/`

Apply these rules in order:

1. Replace executable `mc` with `monochange`.
2. Replace each colon-delimited built-in step token with the nested `monochange step <name>` path.
3. For each command name defined in `monochange.toml` under `[cli.<name>]`, replace `monochange <name>` with `monochange run <name>`.
4. Do not rewrite built-ins such as `monochange check`, `monochange run change`, `monochange init`, `monochange mcp`, `monochange run release`, or `monochange versions` into `monochange run ...` unless that exact name is intentionally a configured command in the repository.
5. Prefer `monochange versions --format json` for machine-readable version output.

## Validation

After updating automation, run the commands that the repository expects agents or CI to use. A typical monochange repository can validate with:

```sh
monochange check
monochange step validate
monochange versions --format json
```
