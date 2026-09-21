---
"monochange": minor
---

# Group the publish steps behind `monochange publish`

The publishing steps were only reachable through long `monochange step *` invocations, which made them hard to discover and awkward to type. The three publishing operations are now also available as subcommands of a built-in `monochange publish` command:

```bash
# before
monochange step publish-packages --output publish.json
monochange step publish-readiness --from HEAD --output readiness.json
monochange step placeholder-publish --format json

# after (equivalent)
monochange publish packages --output publish.json
monochange publish readiness --from HEAD --output readiness.json
monochange publish placeholder --format json
```

Each subcommand runs the exact same built-in step with the same inputs and output formats, so `--format`, `--output`, `--from`, `--package`, `--group`, `--ecosystem`, `--resume`, `--all`, `--show-all`, `--stream-output`, `--fail-on-duplicate`, and `--otp` keep working unchanged. Diagnostics name the command you typed, so a failure reports `command: monochange publish readiness` rather than the bare step name.

The `monochange step *` forms remain supported and unchanged. Config-defined commands are unaffected: a `[cli.publish]` workflow in `monochange.toml` still runs as `monochange run publish`, because config commands and built-in command groups are separate namespaces.
