# Human-first CLI output and release-note audit

## Status

- Audit date: 2026-09-08
- Scope: CLI results, progress, diagnostics, CI logs, changeset authoring, and changelog rendering
- Outcome: output contract approved; implementation is in progress
- Code changes: PR 1 process-contract and PR 2 human-default work are complete

## Assessment

The CLI does not have one output contract. Each command decides what `text`, `markdown`, and `json` mean. Three progress reporters make separate decisions about animation, color, terminal control sequences, and CI. Release-note documents also store rendered Markdown entries instead of structured entries, so a later renderer cannot produce clean text or JSON.

These are product-model problems. More styling would hide the symptoms for a short time, but it would not make results easier to understand.

The first work should fix correctness and terminal hygiene. The next work should make each command answer one question first: what happened, what matters, and what the user can do next. Release-note rendering can then use the same rule: show the important change first and move provenance or internal detail after it.

## Recommended contract

Every command should follow this contract:

1. The default result is concise human-readable text.
2. `--format text` returns the same semantic content in a terminal and in CI. A terminal can add color, but it cannot change the wording or hierarchy.
3. `--format markdown` always returns raw Markdown. It does not become terminal-rendered text when stdout is a TTY.
4. `--format json` and `--format json-min` return structured, ANSI-free data only when the caller asks for them.
5. stdout contains the result. stderr contains progress and diagnostics.
6. The output format never changes the exit status.
7. `--quiet` changes output only. It never turns a real operation into a dry run.
8. Animation requires an interactive terminal. Captured output and CI use complete, deterministic lines with no cursor control sequences.
9. `NO_COLOR`, `MONOCHANGE_NO_PROGRESS`, and a future `--no-progress` flag apply to every command and nested operation.
10. A failure says what failed, why it failed, where it failed, and what the user can do next. `--log-level debug` remains an implementation trace, not the normal way to understand a failure.

## Evidence from the current CLI

The commands below used `target/debug/monochange` in this checkout. Publish inspection used `--dry-run`; no package was published.

| Area                   | Command or source                                                                    | Observed result                                                                     | Consequence                                                          |
| ---------------------- | ------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Exit status            | `monochange check --format json` against `fixtures/tests/check-output/npm-workspace` | Returned status 0 with `error_count: 6` and `warning_count: 1`                      | CI can pass when lint errors exist                                   |
| Progress opt-out       | `MONOCHANGE_NO_PROGRESS=1 TERM=dumb monochange check --format text`                  | Printed the same lint progress as a normal run, including `ESC[2K`                  | The advertised opt-out does not work for `check`                     |
| Captured progress      | `TERM=dumb monochange step validate`                                                 | Printed `\r`, `ESC[2K`, and `ESC[0m` on non-TTY stderr                              | CI logs and captured output contain terminal control bytes           |
| Placeholder publishing | `monochange step placeholder-publish --dry-run --package monochange --format text`   | Reported `1 expected, 0 succeeded, 0 failed, 1 skipped`, then `no packages matched` | The result contradicts itself and hides the package that was checked |
| Config output          | `monochange step config`                                                             | Printed 2,673 lines and 82,638 bytes of pretty JSON                                 | The default is a data dump, not an answer                            |
| Format fidelity        | `monochange step config --format text`                                               | Output was byte-for-byte identical to the pretty JSON result                        | `text` does not mean text for this command                           |
| Help                   | `monochange help step placeholder-publish`                                           | `--package`, `--show-all`, and `--otp` had no descriptions                          | Users cannot predict filtering or the final result                   |
| Changeset style        | `docs/agents/changeset-quality.md` and `monochange.toml`                             | The guide requires an H4 headline; generated files and the lint config require H1   | Authors receive incompatible instructions                            |

### Reproduce the exit-status defect

```bash
audit_root="$(mktemp -d)"
cp -R fixtures/tests/check-output/npm-workspace/. "$audit_root"
(
	cd "$audit_root"
	/absolute/path/to/target/debug/monochange check --format json
)
echo "$?"
```

The JSON branch in `run_check_command` returns an error only when workspace validation fails. The text branch also checks `lint_has_errors`. This format-specific branch causes the status mismatch.

### Reproduce the progress defects

```bash
TERM=dumb MONOCHANGE_NO_PROGRESS=1 \
	target/debug/monochange check --format text \
	>/tmp/monochange-check.out \
	2>/tmp/monochange-check.err

TERM=dumb \
	target/debug/monochange step validate \
	>/tmp/monochange-validate.out \
	2>/tmp/monochange-validate.err
```

Inspect the stderr files with a control-character-aware viewer. Both paths can emit cursor-clearing sequences even though neither command writes to an interactive terminal.

## Root causes

### Output format is chosen too late and means different things

`detect_output_format_from_env_args` defaults to `OutputFormat::Markdown`. `run_from_env` then asks `termimad` to transform that Markdown when stdout is a TTY. As a result, one command can produce rendered terminal text or raw Markdown based on its destination.

`cli_command_output_format` also defaults configured commands to Markdown. Some built-in commands default to text, while `step config` is special-cased to JSON. Several renderers handle `Text` and `Markdown` with the same function, and `render_config_step_json` serializes JSON for every format. The enum therefore describes neither a stable wire format nor a stable presentation format.

`CliContext.last_step_inputs` helps choose the final format. This makes the final renderer depend on whichever step last stored inputs. The command boundary should own the format for the whole invocation.

### Progress is implemented three times

The CLI has separate implementations for workflow progress, lint progress, and publish progress:

- `cli_progress.rs` manages workflow steps, spinner state, ANSI styles, CI output, and NDJSON events.
- `lint_check_reporter.rs` creates a second spinner and color policy.
- `publish_progress.rs` creates a third terminal and CI policy, then coordinates with the main spinner through shared state.

The implementations do not agree on CI environment variables or when to clear a line. `CliProgressReporter::print_line` always writes cursor-clearing bytes. `HumanLintProgressReporter::new` computes an `enabled` value but stores only the derived color value. Its callbacks keep printing after progress has been disabled.

This split explains the color bleed and broken progress. A style reset in one reporter cannot reliably protect output from another reporter or a nested subprocess.

### Progress starts after expensive setup

`run_with_args_in_dir` loads the workspace configuration and builds the configured CLI before `execute_cli_command_with_options` creates `CliProgressReporter` and calls `command_started`. A slow configuration or package-discovery failure therefore occurs before the normal workflow reporter exists.

`Validate` also reports one broad step. In this checkout it took about 11.6 seconds without showing which validation phase was active. A progress line exists, but it does not answer what the command is doing.

### Debug tracing is compensating for missing product diagnostics

`tracing_setup::init_tracing` installs no subscriber unless the caller passes `--log-level`. When enabled, tracing includes targets and span-close events. This is useful for a maintainer, but it is not a user-facing explanation.

Operational failures are often converted into `MonochangeError::Config(String)`. `run_cli_binary_from_env` prints only `error.render()`. This loses structured context such as the affected package, the failed phase, a stable error code, and a suggested command. It also produces messages such as `config error: check failed` when the problem is a lint result rather than configuration.

### Publish summaries collapse different states into “skipped”

`PackagePublishReport::summary` counts only `Published` as success and `Failed` as failure. Every other status, including `Planned`, becomes `skipped`. A dry run can therefore say that work was skipped when it was actually planned.

`filter_placeholder_publish_report` removes `SkippedExisting` rows by default. The renderer then sees an empty filtered list and says that no package matched, while the summary still uses the complete report. The package matched, but the registry check found that it already existed.

### Release-note structure is discarded before rendering

`ReleaseNoteChange` contains useful structured fields. `render_release_note_sections` immediately turns each change into a rendered `String` and stores `Vec<String>` in `ReleaseNotesSection`. JSON output then contains Markdown blobs instead of fields such as `summary`, `details`, `packages`, and `review_request`.

The text renderer cannot recover the lost structure. It strips some Markdown prefixes but leaves Markdown emphasis and metadata syntax. `ChangelogFormat::Monochange` and `ChangelogFormat::KeepAChangelog` also have almost the same body structure, so the configured format does not buy a meaningful presentation choice.

### Authoring rules and rendered rules are mixed

Changeset source files use an H1 summary because `render_changeset_markdown` writes `#` and the repository config enforces `heading_level = 1`. The changeset guide asks authors for `#### short title`, which is the current changelog template's rendered level.

An author should write a semantic summary, not guess the heading depth of a future changelog. The changelog renderer should choose the final heading or list level.

The guide also requires usage examples in every changeset. That rule makes small fixes and internal changes verbose. Examples are valuable for a changed command, API, configuration, or migration. They are noise when there is nothing for a user to copy or change.

## Proposed human output

### Lead with the outcome

The first non-progress line should answer the command's main question. Counts, lists, and next steps should follow in that order. Do not start every result with `command ... completed`; the exit status already says that the process ended.

Use progressive disclosure:

- Default: outcome, important items, warnings, and next action.
- `--verbose`: per-item status, commands, captured subprocess output, and timing detail.
- `--format json`: the complete structured report.
- `--quiet`: no result or progress, with unchanged execution semantics.

### `placeholder-publish`

Current output:

```text
command `step placeholder-publish` completed (dry-run)
placeholder publishing:
  summary: 1 expected, 0 succeeded, 0 failed, 1 skipped
- no packages matched the publishing criteria
publish rate limits:
- no publish operations matched the current plan
```

Proposed dry-run output when two packages need placeholders:

```text
Would publish 2 placeholder packages

  @scope/ui  0.0.0  npm
  monochange 0.0.0  crates.io

18 packages already exist. No changes were made.
```

Proposed output when every placeholder already exists:

```text
No placeholder packages need publishing

Checked 18 packages. All placeholder versions already exist.
```

Proposed output after a real run:

```text
Published 2 placeholder packages

  @scope/ui  0.0.0  npm
  monochange 0.0.0  crates.io

18 already existed. 0 failed.
```

Use explicit counters for `planned`, `published`, `already_exists`, `blocked`, `failed`, and `not_attempted`. Do not map all non-success states to `skipped` in the domain summary.

### `check`

Proposed success output:

```text
Checks passed

Validated the workspace and ran 4 lint suites in 1.8s.
```

Proposed failure output:

```text
Checks failed: 6 errors, 1 warning

package.json
  8:10  error  npm/workspace-protocol
        Use `workspace:^` for internal dependencies.

2 errors can be fixed automatically.
Run: monochange check --fix
```

The human error renderer should not wrap this result in `config error: check failed`. The JSON report should contain the same counts and diagnostics, and the process should exit with status 1.

### `step config`

Proposed default output:

```text
Workspace configuration

Path: monochange.toml
Packages: 36 (26 cargo, 10 npm)
Version groups: 2
Configured commands: 3
Changelog streams: 1

Use `--format json` to print the complete resolved configuration.
```

`--format json` should keep the existing complete object. `--format json-min` should keep the compact object. `--format text` must not call `render_json_value`.

A later `step config get <path>` command could support focused inspection, but it is not required to fix the default.

### Long-running workflow

Proposed interactive output can update one line:

```text
⠹ Validating changesets: 42 files
```

Proposed CI output uses complete lines:

```text
[1/4] Load configuration... done (240ms)
[2/4] Validate 42 changesets... done (1.2s)
[3/4] Check versioned files... done (310ms)
[4/4] Run 4 lint suites... failed (2.8s)
```

The final error follows those lines. CI does not receive `\r`, `ESC[2K`, a spinner frame, or a partial line.

### Actionable errors

Use a small structured diagnostic type:

```rust
struct CliDiagnostic {
	code: &'static str,
	severity: DiagnosticSeverity,
	summary: String,
	context: Vec<DiagnosticContext>,
	hints: Vec<String>,
}
```

A human renderer could produce:

```text
error[check.failed]: Checks failed with 6 errors
  at package.json
  help: Run `monochange check --fix` to fix 2 errors automatically.
```

A JSON diagnostic renderer could expose the same fields on stderr when a caller explicitly asks for machine diagnostics. Do not put Rust targets, spans, or internal function names in normal errors.

## Proposed changeset format

Keep the source format plain and semantic:

```markdown
---
monochange: fix
---

# Keep JSON check failures non-zero

`monochange check --format json` now exits with status 1 when lint errors exist. The JSON report remains available on stdout, so CI can inspect the report without treating the check as successful.
```

Use these authoring rules:

- Require one H1 summary in the changeset source.
- Limit the summary to one outcome-focused sentence fragment with no trailing period.
- Require one impact paragraph for public behavior changes.
- Require examples only when an invocation, API, configuration, migration, or output shape changes.
- Reject a first details sentence that repeats the summary after case and punctuation normalization.
- Keep developer and user streams separate, as the current repository rules require.
- Let the renderer choose changelog heading levels and package-label placement.

Update `docs/agents/changeset-quality.md` to match `render_changeset_markdown` and `changesets/summary.heading_level = 1`.

## Proposed changelog format

Ordinary changes should be compact. Breaking changes and migrations should keep enough detail to act without reading the source diff.

```markdown
## 0.11.0 - 2026-09-08

1 breaking change · 3 features · 7 fixes · 12 packages

### Breaking changes

#### Preserve release-note output identity

`PreparedChangelog` now identifies its configured output and stream. Callers that construct the value directly must provide both fields.

_Packages: monochange_core, monochange_changelog · Review: #842_

### Features

- **Add compact JSON output.** Use `--format json-min` for one-line reports.
- **Extract named release notes.** Select one configured artifact with `monochange notes --output <id>`.

### Fixes

- **Keep JSON check failures non-zero.** JSON output now uses the same failure status as text output.
```

Recommended rendering rules:

- Render `breaking` entries as expanded subsections.
- Render `feat`, `change`, `fix`, `security`, and `perf` as compact entries unless the body contains a migration or code block.
- Collapse or omit `test`, `refactor`, and `docs` sections according to the existing priority thresholds.
- Show package labels in grouped or workspace changelogs. Omit them from a package's own changelog.
- Put provenance after the explanation. Do not place `_Packages:_` between a heading and its body.
- Make section emoji opt-in. Reserve default icons and color for terminal status, where they communicate state.
- Add a short release summary only when it provides counts that the document does not repeat elsewhere.

The current template system can approximate compact entries, but it cannot safely lay out arbitrary multiline details inside a bullet. Implement compact and expanded entry styles in the renderer rather than adding more template combinations.

## Proposed data model

Keep release-note entries structured until the final output format is known:

```rust
struct ReleaseNotesSection {
	title: String,
	collapsed: bool,
	entries: Vec<ReleaseNotesEntry>,
}

struct ReleaseNotesEntry {
	summary: String,
	details_markdown: Option<String>,
	packages: Vec<String>,
	change_type: Option<String>,
	stream: String,
	provenance: ReleaseNoteProvenance,
}
```

Then render each format from that model:

- Markdown preserves headings, paragraphs, code blocks, links, and collapsible sections.
- Text removes Markdown syntax intentionally and uses indentation for detail.
- JSON exposes fields instead of embedding a complete rendered entry in one string.
- Provider adapters can request Markdown without parsing a changelog file.

This migration changes public types in `monochange_core`. Stage it behind new entry types and compatibility conversion methods before removing `Vec<String>`.

## Smallest useful architecture change

Do not start by rewriting `cli_runtime.rs`. First create one output boundary and route existing renderers through it.

```text
command execution
  -> structured result or structured diagnostic
  -> output renderer selected once at the command boundary
  -> stdout writer

command, step, lint, publish, and subprocess events
  -> one progress reporter
  -> stderr writer with one terminal capability snapshot
```

The terminal capability snapshot should include:

- stdout and stderr TTY state
- CI state
- color enabled
- animation enabled
- progress enabled
- Unicode enabled
- result format
- progress format

All reporters should use the same locked stderr writer. Only the interactive renderer may clear or rewrite a line. Every style helper must reset its style in the same write.

After that boundary exists, split `cli_runtime.rs` by responsibility:

- Keep workflow orchestration in `cli_runtime.rs`.
- Move result rendering to `output/result.rs`.
- Move the shared progress reporter to `output/progress.rs`.
- Move terminal capability detection and the locked writer to `output/terminal.rs`.
- Keep domain-specific result construction near `lint`, `package_publish`, `release_artifacts`, and `changesets`.

## Ordered implementation plan

### PR 1: Lock the process contract and fix correctness

- [x] Add binary-level integration tests that capture stdout, stderr, and exit status.
- [x] Assert that `check` exits with the same status for text, Markdown, JSON, and compact JSON.
- [x] Fix the JSON branch in `run_check_command` to fail on `lint_has_errors`.
- [x] Add tests for `NO_COLOR`, `MONOCHANGE_NO_PROGRESS`, `TERM=dumb`, a common CI variable, and non-TTY stderr.
- [x] Stop both existing progress reporters from writing cursor controls outside an interactive renderer.
- [x] Store and honor `enabled` in `HumanLintProgressReporter`.

Acceptance checks:

- A lint error exits 1 for every result format.
- Captured stdout and stderr contain no ANSI or cursor-control bytes.
- `MONOCHANGE_NO_PROGRESS=1` produces no progress for `check`, `lint`, or configured workflows.

### PR 2: Make the default result human-readable

- [x] Change the default command result from Markdown to text.
- [x] Remove terminal transformation from explicit `--format markdown`.
- [x] Select result format once at the command boundary instead of reading `last_step_inputs`.
- [x] Add a human summary renderer for `step config`.
- [x] Make `--jq` require or imply JSON input with a clear help message.
- [x] Decouple `--quiet` from `dry_run` and add a changeset that calls out the behavior change.
- [x] Describe every built-in step input in `step_inputs_schema`.

Acceptance checks:

- Running a command with no format never prints JSON unless JSON is the command's explicit domain result.
- `--format text`, `--format markdown`, and `--format json` each have one literal meaning.
- `--quiet` performs the same operation as the unquiet command.

### PR 3: Make publish outcomes obvious

- [ ] Replace the four-field publish summary with per-status counts.
- [ ] Render `planned` as “would publish” in a dry run.
- [ ] Render `SkippedExisting` as “already exists,” not as “did not match.”
- [ ] Put the published or planned package list immediately below the headline.
- [ ] Move registry commands, stdout, stderr, trust details, and individual skipped rows behind `--verbose` or `--show-all`.
- [ ] Keep a complete structured report for JSON callers.

Acceptance checks:

- A user can identify published packages without scrolling.
- Empty, all-existing, partially successful, failed, blocked, and dry-run outcomes have distinct headlines.
- No report says that zero packages matched when at least one package was checked.

### PR 4: Unify progress and diagnostics

- [ ] Introduce one terminal capability snapshot and one stderr writer.
- [ ] Route workflow, lint, publish, and subprocess events through one progress reporter.
- [ ] Create phase events before workspace configuration loads and during validation.
- [ ] Keep interactive animation and deterministic CI rendering as two views of the same event.
- [ ] Add structured CLI diagnostics with stable codes, context, and hints.
- [ ] Keep tracing opt-in and document it as maintainer diagnostics.

Acceptance checks:

- A command that runs for more than one second names its active phase.
- A nested publish or lint operation cannot overwrite another reporter's line.
- Errors include a cause and a next action when monochange can provide one.

### PR 5: Separate release-note data from rendering

- [ ] Add `ReleaseNotesEntry` and structured provenance to `monochange_core`.
- [ ] Convert `ReleaseNoteChange` to structured entries without rendering Markdown.
- [ ] Render Markdown, text, and JSON only at the final boundary.
- [ ] Add compact and expanded changelog entry styles.
- [ ] Make grouped and package changelogs choose package-label visibility from context.
- [ ] Align the changeset guide, generator, lint defaults, and examples on an H1 source summary.
- [ ] Add a lint for summary and first-paragraph duplication.

Acceptance checks:

- JSON release notes contain fields, not Markdown documents embedded as strings.
- Text release notes contain no Markdown emphasis markers.
- Package changelogs do not repeat their own package name on every entry.
- Breaking entries remain actionable; routine fixes fit on one or two lines.

### PR 6: Consolidate the existing plans

- [ ] Move completed progress and output plans from `docs/plans/active` to `docs/plans/completed`.
- [ ] Fold remaining publish work into this plan or link the plans with explicit ownership.
- [ ] Mark shipped `json-min` work complete.
- [ ] Remove plan statements that conflict with the final output contract.

## Test matrix

Every major command family should have at least one success and one failure scenario at the process boundary.

| Result format | stderr destination | Environment                | Assertions                                               |
| ------------- | ------------------ | -------------------------- | -------------------------------------------------------- |
| default text  | TTY                | local                      | concise result, scoped color, animation allowed          |
| default text  | pipe               | local                      | same words, no ANSI, complete progress lines             |
| default text  | pipe               | CI                         | deterministic progress, useful timing, no cursor control |
| text          | pipe               | `TERM=dumb`                | plain text only                                          |
| text          | pipe               | `NO_COLOR=1`               | no ANSI                                                  |
| text          | pipe               | `MONOCHANGE_NO_PROGRESS=1` | empty progress stderr                                    |
| Markdown      | any                | any                        | raw valid Markdown, no terminal transformation           |
| JSON          | pipe               | any                        | one valid JSON document on stdout                        |
| compact JSON  | pipe               | any                        | one valid JSON document on one line                      |
| any           | any                | failure                    | identical non-zero status for the same domain failure    |

Use the real binary in `crates/monochange_integration_tests`. Tests that call `run_with_args_in_dir` remain useful for renderer logic, but they cannot prove process exit status, stdout and stderr separation, TTY behavior, or control-byte hygiene.

## Validation commands

Run repository commands in the devenv shell:

```bash
devenv shell dprint check docs/plans/active/cli-output-and-release-notes-audit.md
devenv shell cargo test -p monochange --lib
devenv shell cargo test -p monochange_integration_tests
devenv shell lint:all
devenv shell monochange step validate
```

For each PR that changes executable lines, also run the repository's patch-coverage check.

## Risks and boundaries

- Human output is not a stable parsing API. JSON schemas and exit codes are.
- Changing the default from Markdown to text changes snapshots and scripts that parse undocumented human output. Announce the change in a developer changeset.
- Decoupling `--quiet` from dry-run can expose callers that relied on the current coupling. Add an explicit `--dry-run` to those workflows before the behavior changes.
- Structured release-note entries affect public Rust types. Use an additive migration before a removal.
- Do not make progress quieter in CI. Make it deterministic and more specific.
- Do not print subprocess output by default on success. Preserve it for failures and expose it through `--verbose`.
- Do not add a large rendering framework. A small structured document model and two writers are enough.

## Files that own the work

- `crates/monochange/src/lib.rs`: process boundary, default format, error exit, and quiet semantics
- `crates/monochange/src/cli.rs`: help text and global output flags
- `crates/monochange/src/cli_runtime.rs`: workflow orchestration and current result renderers
- `crates/monochange/src/cli_progress.rs`: current workflow progress renderer
- `crates/monochange/src/lint_check_reporter.rs`: current lint reporter
- `crates/monochange/src/publish_progress.rs`: current publish reporter
- `crates/monochange/src/lint.rs`: check result and exit-status logic
- `crates/monochange/src/tracing_setup.rs`: maintainer tracing
- `crates/monochange_publish/src/lib.rs`: publish status model and summary
- `crates/monochange_changelog/src/lib.rs`: changelog entry construction and rendering
- `crates/monochange_core/src/lib.rs`: release-note document types
- `crates/monochange_config/src/lib.rs`: changeset parsing and heading normalization
- `crates/monochange/src/changesets.rs`: changeset source generation
- `docs/agents/changeset-quality.md`: authoring guidance
- `crates/monochange_integration_tests`: process-level contract tests

## Decision summary

Adopt text as the default result, keep Markdown and JSON explicit, and make stdout a concise answer instead of a workflow transcript. Unify progress before adding more animation or styling. Fix the format-dependent exit status and control-byte leaks first. Model publish outcomes and release-note entries as data so every renderer can emphasize the information its audience needs.
