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

The human renderer is designed for interactive terminal runs:

- configuration loading and validation report their active phase before work begins
- step labels use each step's `name = "..."` value when present, then fall back to the built-in step kind
- long-running steps show a delayed spinner so short steps do not flicker
- command stdout and stderr stream live under the active step
- completed `PrepareRelease` and `DisplayVersions` steps print per-phase timings so slow phases are visible without a separate trace

The same events become complete, newline-terminated records when stderr is captured or monochange runs in CI. Lint and publish operations share the workflow reporter, so a nested operation cannot create a second spinner or append text to an active line.

Built-in commands already attach descriptive step names such as `prepare release`, `publish release`, and `open release request`. Custom commands can override those names per step.

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

Example:

```json
{"sequence":0,"event":"phase_started","phase":"Loading workspace configuration"}
{"sequence":1,"event":"phase_finished","phase":"Loaded workspace configuration","duration_ms":12}
{"sequence":2,"event":"command_started","command":"release","dry_run":true,"total_steps":2}
{"sequence":3,"event":"step_started","command":"release","dry_run":true,"step_index":1,"total_steps":2,"step_kind":"PrepareRelease","step_display_name":"plan release","step_name":"plan release"}
{"sequence":4,"event":"step_finished","command":"release","dry_run":true,"step_index":1,"total_steps":2,"step_kind":"PrepareRelease","step_display_name":"plan release","step_name":"plan release","duration_ms":243,"phase_timings":[{"label":"discover release workspace","duration_ms":97}]}
```

## Failure diagnostics and maintainer tracing

Normal failures use a stable diagnostic code and put the useful recovery information first:

```text
error[cli.json_required]: --jq requires explicit JSON output
  command: monochange step config
  help: Add `--format json` or `--format json-min` before using `--jq`.
```

Use the code when searching CI logs or reporting a recurring failure. The diagnostic includes command or path context when monochange knows it and a next action when it can recommend one.

`--log-level <FILTER>` enables local maintainer tracing, for example `--log-level debug` or `--log-level monochange=trace`. Tracing is opt-in, may include internal spans and implementation detail, and is not the normal user-facing explanation for a failure. Animation is disabled while tracing is active so trace records and progress lines remain readable. This flag does not enable remote telemetry.

## Benchmark integration

The binary benchmark workflow uses `--progress-format json` to extract `PrepareRelease` phase timings for both `monochange run release --dry-run` and `monochange run release`.

Those timings are summarized and compared against `scripts/benchmark-phase-budgets.json`, which lets pull requests fail when real release-path regressions exceed the configured budget.

For hosted-provider analysis outside CI, `pnpm node scripts/benchmark-cli.ts run-fixture` can benchmark an existing repository checkout and render the same markdown summary against a real hosted fixture. See [Hosted release benchmarks](./hosted-release-benchmarks.md).
