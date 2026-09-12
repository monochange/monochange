# Package CLI registration

A package can ship a CLI binary in addition to its library surface. Register that CLI under `[package.<id>].cli` so monochange knows the binary name, can capture a normalized snapshot of its command surface, and can classify command-surface breaks during `monochange change classify` instead of reporting those changes as unclassified package changes.

## Register a CLI

```toml
[package.monochange]
path = "crates/monochange"
cli = { name = "monochange", snapshot = "monochange snapshot --view index" }
```

Both fields are required:

| Field      | Meaning                                                                                                          |
| ---------- | ---------------------------------------------------------------------------------------------------------------- |
| `name`     | The binary name users invoke. It also names the committed baseline file and must be unique across the workspace. |
| `snapshot` | A command that prints a normalized command-surface snapshot JSON document on stdout.                             |

`snapshot` accepts either a bare command string or a detailed definition with the same fields as `[ecosystems.*].lockfile_commands` entries:

```toml
[package.some_cli]
cli = { name = "somecli", snapshot = { command = "node scripts/emit-cli-snapshot.mjs", cwd = "packages/somecli", shell = false } }
```

`cwd` is workspace-relative when set; without it the command runs from the workspace root. `shell = true` runs the command through `sh -c`. CLI registration is additive to `package_type`: the package keeps its ecosystem surface and public-API analysis and gains a command-surface identity on top. One CLI per package; register the package that owns the binary source, not platform wrappers that repackage it.

## Snapshot contract

The snapshot command must print one `CommandSnapshot` JSON document (see the `monochange_snapshot` crate). For CLIs built with clap in this repository, `monochange snapshot --view index` already emits it. For foreign CLIs, commit a small emitter script that maps the CLI's help or command metadata into the snapshot schema. monochange validates `schema_version` against its supported snapshot schema version and rejects stale or unparsable documents.

## Baselines

Captured snapshots are committed release state at `.monochange/cli-snapshots/<name>.json`. Refresh them as part of the release workflow so the baseline always describes the latest release:

```toml
[cli.release]
steps = [
	{ type = "Command", name = "capture cli snapshot", when = "{{ number_of_changesets > 0 }}", command = "monochange snapshot --package <id> --save" },
]
```

The diff compares command surface only and ignores `tool.version`, so baselines do not need to be rebuilt at the exact released commit.

Useful commands:

- `monochange snapshot` — print monochange's own surface (unchanged behavior).
- `monochange snapshot --package <id>` — run that package's configured snapshot command and print the document.
- `monochange snapshot --package <id> --save` — write or update the committed baseline.
- `monochange snapshot --list` — list registered CLIs and baseline status.

## Classification integration

For every classified package with a registered CLI whose files changed, `monochange change classify` captures a fresh snapshot, diffs it against the committed baseline, and appends first-class findings:

- analyzer id `monochange/cli-surface`, rule ids `monochange/cli-surface/<change-kind>`, `surface: "cli"`;
- removed commands, options, or positionals and value narrowing propose `major` (breaking); additions and widening propose `minor` (additive); description-only changes are compatible patches;
- per-command `max_bump` caps from the snapshot still apply;
- findings are high confidence with complete coverage, so they raise `enforceableMinimum` and clear the "unknown impact" review requirement that unclassified changes otherwise trigger.

Each classified package may also carry an additive `cli` block in the report: `{ name, status, recommendation?, findingCount?, baseline? }` with status `diffed`, `missing_baseline`, `stale_baseline`, `failed`, or `skipped`. Capture problems degrade to warnings so the pull request comment still posts.

Skip the comparison with `--skip-cli-snapshots` or `MONOCHANGE_SKIP_CLI_SNAPSHOTS=1` (useful when the snapshot command needs a build that is unavailable in a given environment).
