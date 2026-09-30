---
"monochange": feat
---

# Make workflow progress readable in terminals and CI logs

Long release workflows were hard to follow: every step printed twice (`▶ [2/9] step` and `▶ [2/9] step — running command …`), a silent external command added a "still running" line every five seconds (70 lines for one six-minute step in the release pull request job), and single-step commands such as `monochange next` wrapped one step line in a command banner and footer.

Progress output now:

- lines step results up in a column and gives unnamed built-in steps readable labels (`calculate next versions` instead of `DisplayVersions`)
- shows the command a `Command` step runs as `$ <command>` under the step
- prints a banner and a `<command> completed in 7m 36s` summary only for multi-step commands
- marks dry-run `Command` steps as skipped instead of successful, because the command did not run
- shows a live elapsed timer on the interactive spinner, and in captured or CI logs notes a silent command after 30 seconds and then once a minute
- lists phase timings only for steps slower than one second
- folds each step's command output into a collapsible `::group::` when `GITHUB_ACTIONS=true`, and turns warnings and the final failure into `::warning`/`::error` annotations

```text
monochange › release · 3 steps
▶ [2/3] generate cargo lockfile
  $ cargo generate-lockfile
::group::[2/3] generate cargo lockfile · output
  │     Updating crates.io index
::endgroup::
✔ [2/3] generate cargo lockfile          937ms
```

`--progress-format json` events are unchanged, apart from heartbeat and warning timing.
