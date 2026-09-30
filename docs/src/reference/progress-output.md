# Progress output

monochange writes progress information to stderr so stdout can remain stable for text, markdown, and JSON command results.

## Selecting a renderer

Use the global `--progress-format <FORMAT>` flag or set `MONOCHANGE_PROGRESS_FORMAT`.

Supported values:

- `auto`: default behavior. Deterministic human progress output is enabled for terminals, CI logs, editor tasks, and captured processes; animation is terminal-only.
- `unicode`: force the human renderer with Unicode symbols and spinners.
- `ascii`: force the human renderer with ASCII-safe symbols.
- `json`: emit newline-delimited JSON progress events on stderr.

`--quiet` suppresses progress output. `MONOCHANGE_NO_PROGRESS=1` also disables the automatic human renderer.

## Human progress output

The human renderer is designed to be read, both in an interactive terminal and in CI logs:

- configuration loading and validation report their active phase before work begins
- step labels use each step's `name = "..."` value when present; unnamed built-in steps get a readable label such as `calculate next versions` or `prepare release`
- step results line up in a column with their duration, and named built-in steps keep their kind, such as `(PrepareRelease)`, so a label can be traced to its step reference
- commands with more than one step open with a `monochange › <command> · <n> steps` banner and close with `<command> completed in <duration>` or `<command> failed after <duration>`; single-step commands show only the step
- a `Command` step shows the command it runs as `$ <command>` under the step instead of repeating the step line
- command stdout and stderr stream under the active step, prefixed with `│`; stdout and stderr interleave in arrival order, so use `--progress-format json` when a consumer must tell the two streams apart
- dry-run `Command` steps show as skipped, because the command was not run
- steps slower than one second list their slowest phases, so slow phases are visible without a separate trace
- the interactive spinner shows how long a slow step has been running; captured and CI logs instead note that a silent command is still running after 30 seconds and then once a minute

Captured command output looks like this:

```text
monochange › release · 3 steps
▶ [1/3] plan release (PrepareRelease)
✔ [1/3] plan release (PrepareRelease)  1.23s
    · enrich changeset context via github  896ms
    · build release plan                   151ms
▶ [2/3] format release files
  $ dprint fmt
  │ Formatted 54 files.
✔ [2/3] format release files           2.70s
▶ [3/3] run tests
  $ cargo test
  │ test a ... FAILED
✖ [3/3] run tests                      41.2s
  └─ command failed (exit status: 101)
✖ release failed after 45.2s
```

The same events become complete, newline-terminated records when stderr is captured or monochange runs in CI. Lint and publish operations share the workflow reporter, so a nested operation cannot create a second spinner or append text to an active line. Publish progress uses the same symbols, colors, and ASCII fallback as workflow progress.

### GitHub Actions

When `GITHUB_ACTIONS=true`, the human renderer also uses GitHub workflow commands:

- each step's command output is folded into a collapsible `::group::` titled after the step, so long build logs do not bury the step results
- warnings become `::warning` annotations and a failure adds an `::error` annotation, so both appear in the run summary and on the pull request
- captured command output keeps its `│` prefix, so a workflow command printed by a nested tool is never executed by the runner

## Warnings

monochange prints warnings without `--log-level`. A warning such as a GitHub API fallback or a publish retry appears once as a `warning:` line with its details underneath:

```text
warning: could not create a verified release commit through the GitHub API; falling back to a regular git commit
  reason: GitHub API POST `/repos/acme/app/git/trees` failed: status 422
  commit: c686e78478ea2611d41e2d8521311a236f2a3470
```

`--quiet` hides warnings. With `--progress-format json`, a warning is a `warning` event with `message` and `fields`.

## JSON event stream

`--progress-format json` is intended for machines, not humans. It writes one JSON object per line to stderr.

Common lifecycle events:

- `phase_started`
- `phase_finished`
- `phase_failed`
- `command_started`
- `step_started`
- `command_output`
- `step_finished`
- `step_failed`
- `step_skipped`
- `command_finished`
- `command_failed`
- `lint_planning_started`
- `lint_planning_finished`
- `lint_suite_started`
- `lint_suite_finished`
- `lint_fix_started`
- `lint_fix_applied`
- `lint_fix_finished`
- `lint_summary`
- `publish_run_started`
- `publish_registry_check_started`
- `publish_package_started`
- `publish_package_skipped`
- `publish_package_planned`
- `publish_package_published`
- `publish_package_failed`
- `publish_run_finished`

Shared fields:

- `sequence`: monotonically increasing event sequence number for the command run
- `command`: CLI command name, such as `release`
- `dry_run`: whether the command is running in dry-run mode
- `total_steps`: total step count for the command
- `step_index`: 1-based step index for step events
- `step_kind`: built-in step kind, such as `PrepareRelease`
- `step_display_name`: rendered human label for the step
- `step_name`: explicit configured `name`, or `null` when omitted

Event-specific fields:

- `command_output` adds `stream` and `text`
- `phase_finished`, `step_finished`, and command completion events add `duration_ms`
- `step_finished` adds `phase_timings`
- `step_failed` adds `duration_ms` and `error`
- `step_skipped` may add the backward-compatible `condition` field for conditional skips and `reason` for a human-readable explanation
- `command_failed` adds `duration_ms` and `error`
- `warning` adds `message`; warnings raised inside monochange's libraries also add `fields`

Example:

```json
{"sequence":0,"event":"phase_started","phase":"Loading workspace configuration"}
{"sequence":1,"event":"phase_finished","phase":"Loaded workspace configuration","duration_ms":12}
{"sequence":2,"event":"command_started","command":"release","dry_run":true,"total_steps":2}
{"sequence":3,"event":"step_started","command":"release","dry_run":true,"step_index":1,"total_steps":2,"step_kind":"PrepareRelease","step_display_name":"plan release","step_name":"plan release"}
{"sequence":4,"event":"step_finished","command":"release","dry_run":true,"step_index":1,"total_steps":2,"step_kind":"PrepareRelease","step_display_name":"plan release","step_name":"plan release","duration_ms":243,"phase_timings":[{"label":"discover release workspace","duration_ms":97}]}
```

## Failure diagnostics and maintainer tracing

Failures start with a stable diagnostic code, then explain the cause, where it happened, and what to do next:

```text
error[step.command_failed]: command `cargo test` failed: exit status: 101

    stderr (last 20 of 250 lines, full output above):
    test a ... FAILED

  command: monochange run release
  step:    [3/4] run tests
  help:    Fix the failure shown in the command output, then rerun. To reproduce it on its own, run the command directly from the workspace root.
```

Use the code when searching CI logs or reporting a recurring failure. Common codes:

- `cli.usage`: the command line could not be parsed; the diagnostic keeps the usage line and suggestions, and points `monochange <name>` at `monochange run <name>` when `<name>` is defined in `monochange.toml`
- `config.invalid`, `config.parse_failed`, `config.unknown_package`: `monochange.toml` or a changeset target needs fixing; source snippets show the exact location
- `step.command_failed`: a `Command` step exited unsuccessfully; when its output already streamed above, only the last lines are repeated
- `check.failed`, `command.failed`: the result printed above lists the failing items

Paths inside the workspace are shown relative to its root. When `monochange.toml` cannot be loaded, the configuration error is reported even if the failure surfaced while parsing a command defined in that file.

`--log-level <FILTER>` enables local maintainer tracing, for example `--log-level debug` or `--log-level monochange=trace`. Tracing is opt-in, may include internal spans and implementation detail, and is not the normal user-facing explanation for a failure. Animation is disabled while tracing is active so trace records and progress lines remain readable. This flag does not enable remote telemetry.

## Benchmark integration

The binary benchmark workflow uses `--progress-format json` to extract `PrepareRelease` phase timings for both `monochange run release --dry-run` and `monochange run release`.

Those timings are summarized and compared against `scripts/benchmark-phase-budgets.json`, which lets pull requests fail when real release-path regressions exceed the configured budget.

For hosted-provider analysis outside CI, `pnpm node scripts/benchmark-cli.ts run-fixture` can benchmark an existing repository checkout and render the same markdown summary against a real hosted fixture. See [Hosted release benchmarks](./hosted-release-benchmarks.md).
