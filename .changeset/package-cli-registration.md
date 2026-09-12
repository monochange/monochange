---
"@monochange/skill": patch
"monochange": minor
"monochange_core": minor
"monochange_config": minor
"monochange_schema": patch
---

# Register package CLIs and classify command-surface breaks

Change classification could not recognize that a package ships a CLI. Changes to a CLI's command surface — renamed commands, removed options, narrowed values — analyzed as unclassified package files and surfaced in pull request comments with an unknown compatibility impact and a review requirement, even when the break was obvious.

Packages can now register the CLI they ship under `[package.<id>].cli`:

```toml
[package.monochange]
path = "crates/monochange"
cli = { name = "monochange", snapshot = "monochange snapshot --view index" }
```

- `name` is the binary name users invoke. It keys the committed baseline at `.monochange/cli-snapshots/<name>.json` and must be unique across the workspace.
- `snapshot` is required. It accepts a command string or a table with `{ command, cwd, shell }` shaped like `[ecosystems.*].lockfile_commands` entries. The command must print a normalized command-surface snapshot JSON document (`monochange_snapshot::CommandSnapshot`) on stdout; foreign CLIs can commit a small emitter script for this.

`monochange change classify` diffs the committed baseline against a fresh capture for every classified package with a registered CLI and appends `monochange/cli-surface` findings: removed commands, options, positionals, and value narrowing propose `major`; additions and widening propose `minor`; description-only changes are compatible patches. The findings are high confidence with complete coverage, so they raise `enforceableMinimum` and clear the unknown-impact review requirement. Per-package reports gain an additive `cli` block with the comparison status (`diffed`, `missing_baseline`, `stale_baseline`, `failed`, `skipped`), and the classification report schema version bumps to 4.

- `monochange snapshot --package <id>` captures a registered CLI's snapshot; `--save` writes the committed baseline; `--list` prints registered CLIs and baseline health.
- `monochange change classify --skip-cli-snapshots` (or `MONOCHANGE_SKIP_CLI_SNAPSHOTS=1`) skips the comparisons when the snapshot command cannot run.
- The committed JSON schema assets regenerate with the new `package_cli` and `cli_snapshot_command` definitions, and `"snapshot"` joins `RESERVED_CLI_COMMAND_NAMES` so `[cli.*]` workflow commands cannot shadow the built-in.
