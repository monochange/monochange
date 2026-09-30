---
"monochange": feat
---

# Report failures and command step results as JSON

With `--progress-format json`, a failure is now a `diagnostic` event on stderr instead of human text, so the stream stays newline-delimited JSON:

```json
{"sequence":7,"event":"diagnostic","command":"release","dry_run":false,"code":"step.command_failed","summary":"command `cargo test` failed: exit status: 101","detail":"stderr (last 20 of 250 lines, full output above):\ntest a ... FAILED","context":{"command":"monochange run release","step":"[3/4] run tests"},"hints":["Fix the failure shown in the command output, then rerun."],"exit_code":1}
```

`--format json` output now includes a `commands` array describing each `Command` step: its `step` name, `id`, the `command` that ran, `status` (`succeeded`, or `skipped` for a dry run), `exit_code`, `stdout`, and `stderr`. The key is additive: release results keep their shape and gain a top-level `commands` field, and a workflow made only of `Command` steps prints `{"command", "dry_run", "commands"}` instead of text.
