---
"monochange": change
---

# Explain command failures with specific codes, context, and hints

Failure output was hard to read and sometimes pointed at the wrong fix. Command-line typos rendered as `error[config.invalid]: error: unrecognized subcommand …` with a hint to check `monochange.toml`; source snippets were re-indented under `cause:` so their carets no longer lined up; a failed `Command` step printed its full output three times and was labelled `workspace.discovery_failed`; and a broken `monochange.toml` made `monochange run <name>` report `unexpected argument '<name>'` instead of the parse error.

Diagnostics now keep the greppable `error[<code>]: <summary>` first line, render multi-line causes and annotated source snippets as blocks, align `command`, `step`, and `path` context, and show workspace paths relative to the root:

```text
error[step.command_failed]: command `cargo test` failed: exit status: 101

    stderr (last 20 of 250 lines, full output above):
    test a ... FAILED

  command: monochange run release
  step:    [3/4] run tests
  help:    Fix the failure shown in the command output, then rerun. To reproduce it on its own, run the command directly from the workspace root.
```

Codes are more specific. Scripts that search CI logs for the old codes should update them:

| Failure                                       | Before                       | After                                   |
| --------------------------------------------- | ---------------------------- | --------------------------------------- |
| unknown command, flag, or value               | `config.invalid`             | `cli.usage`                             |
| failed `Command` step                         | `workspace.discovery_failed` | `step.command_failed`                   |
| missing git ref, shallow clone                | `workspace.discovery_failed` | `git.failed`                            |
| release record lookup                         | `workspace.discovery_failed` | `release.record_failed`                 |
| unknown package in a changeset or `--package` | `config.invalid`             | `config.unknown_package`                |
| `monochange.toml` syntax error                | `config.invalid`             | `config.parse_failed`                   |
| pre-rendered config validation                | `cli.diagnostic`             | `config.invalid` or `changeset.invalid` |

Other improvements:

- `monochange <name>` suggests `monochange run <name>` when `<name>` is defined in `monochange.toml`
- a command group without a subcommand, such as `monochange changeset`, explains that it needs one and lists them
- when `monochange.toml` cannot be loaded, its error is reported even for commands defined in that file
- a failed `Command` step repeats only the last 20 lines of each stream when its output already streamed live
- generic hints are omitted when the message already says what to do, and git failures suggest fetching full history and tags in CI
