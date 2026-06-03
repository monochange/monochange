---
"monochange": major
---

# Move built-in step commands from `step:*` names to `step *` subcommands

Built-in step commands now live under the `monochange step` command namespace instead of using colon-delimited top-level command names. This makes the CLI hierarchy explicit and leaves top-level command names available for stable product commands.

Before:

```sh
monochange step:validate
monochange step:discover --format json
monochange step:prepare-release --dry-run --format json
monochange step:publish-packages --dry-run
```

After:

```sh
monochange step validate
monochange step discover --format json
monochange step prepare-release --dry-run --format json
monochange step publish-packages --dry-run
```

The command behavior, flags, and output formats are otherwise intended to stay the same. Only the command path changes.

## Migration guidance

Update automation, documentation, scripts, and agent instructions by replacing the `step:` prefix with the nested `step` subcommand. For example:

- `monochange step:validate` becomes `monochange step validate`.
- `monochange step:affected-packages --format json --verify` becomes `monochange step affected-packages --format json --verify`.
- `cargo run -p monochange --bin monochange -- step:config --format json` becomes `cargo run -p monochange --bin monochange -- step config --format json`.

This is a breaking CLI change because old `step:*` command names are no longer the canonical command API.
