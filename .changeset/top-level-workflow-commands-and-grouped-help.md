---
"monochange": feat
---

# Run workflow commands from `monochange.toml` at the top level and group the help by task

Commands defined as `[cli.<name>]` in `monochange.toml` now also run as `monochange <name>`, in addition to `monochange run <name>`. A built-in command keeps its name, so a workflow named like a built-in (for example `[cli.change]`) still runs through `monochange run change`. Prefer `monochange run <name>` in scripts: it keeps working if a later release adds a built-in command with the same name.

`monochange --help` now groups commands by task (release, workspace, change analysis, automation) and lists the workflow commands from `monochange.toml` in their own section, noting which ones need `monochange run`. The `next-versions` alias still works but is no longer listed next to `next`.
