# Publish progress and parallel ecosystems

## Status

- The human output, unified progress, and complete sequential outcome work is complete through #629, #663, and #667.
- This active plan now owns only dependency-aware parallel publishing across ecosystem lanes.
- The completed [human-first CLI output and release-note audit](../completed/cli-output-and-release-notes-audit.md) owns the shared terminal and diagnostic contract. Changes here must reuse that boundary instead of adding another reporter.

## Why

`monochange step publish-packages` now reports readable progress on stderr and keeps machine-readable reports on stdout or in files. Publishing still runs as one sequential list, so an npm-heavy run can finish all npm work before crates.io starts even when ecosystems are independent.

The remaining work is safe ecosystem-level parallelism so independent registries can make progress at the same time without breaking dependency order.

## Goals

- Add safe parallel publish lanes by ecosystem.
- Reuse the shared progress reporter and ecosystem presentation metadata already delivered.
- Preserve deterministic CI lines, interactive-only animation, and explicit machine-readable JSON.

## Non-goals

- Do not publish any packages while implementing or validating this work.
- Do not change release planning semantics.
- Do not run same-ecosystem packages in parallel in the first parallel-publish pass.
- Do not replace telemetry; progress output is complementary.

## Affected areas

- `crates/monochange_publish/src/lib.rs`: dependency-aware ecosystem scheduler and stable report ordering.
- `crates/monochange/src/package_publish.rs`: app-layer orchestration and scheduler integration.
- `crates/monochange/src/output/progress.rs`: reuse the shared renderer if parallel lanes need new events.
- `crates/monochange*/src/__tests__/`: focused unit coverage for changed executable lines.
- CLI snapshots/integration tests where human output intentionally changes.

## PR split

### 1. Progress reporter foundation + publish progress

- [x] Add a small progress abstraction with two renderers:
  - interactive terminal renderer with spinner-style status updates,
  - CI/plain renderer with deterministic emoji start/finish lines.
- [x] Add an ecosystem presentation trait or trait-like helper so ecosystems own their emoji and label.
- [x] Emit publish events for:
  - publish run start and completion,
  - ecosystem lane/package start,
  - registry check,
  - skip existing/external,
  - dry-run planned publish,
  - published,
  - blocked/failed.
- [x] Keep progress on stderr and existing reports on stdout/artifacts.
- [x] Add tests for emoji labels, CI/plain output, and publish event sequencing.
- [x] Validate with `cargo fmt`, targeted tests, `cargo clippy -q -p monochange --all-targets --all-features -- -D warnings`, and `devenv shell monochange step validate`.

### 2. Progress across CLI steps

- [x] Emit step-level progress in `cli_runtime`:
  - command workflow start and exactly one success/failure finish,
  - step start before fallible input and condition resolution,
  - exactly one step success/failure/skip finish,
  - skipped steps with `when` or earlier-failure context,
  - failure context before returning errors.
- [x] Enable deterministic default progress on stderr for terminals, CI, editor tasks, and captured processes; only quiet mode, `MONOCHANGE_NO_PROGRESS`, or an explicit per-step opt-out suppresses it.
- [x] Preserve subprocess stderr and stdout in configured command and lockfile command failures, including stdout-only failures.
- [x] Add step-specific concise summaries for discover, validate/check/lint, prepare release, commit/tag/open release request, publish readiness, issue comments, affected packages, and retargeting.
- [x] Ensure JSON output commands remain parseable by keeping progress on stderr.
- [x] Add tests/snapshots for CI/plain stderr formatting where the harness supports it.

### 2a. Complete sequential publish failure summaries

- [x] Preserve one terminal outcome for every package expected by a sequential publish run.
- [x] Report packages not attempted after the first failure as blocked by that failure.
- [x] Derive and display planned, published, already-existing, blocked, failed, and not-attempted totals from the complete report.
- [x] Include aggregate totals and the underlying failed package error in the returned CLI error.
- [x] Propagate quiet mode into nested publish progress.
- [x] Add unit and output coverage for partial success, first failure, blocked tails, resume behavior, and command stdout/stderr diagnostics.

### 3. Parallel publish by ecosystem

- [ ] Build a dependency-aware scheduler that only releases a package when its publish dependencies succeeded or were already published/skipped existing.
- [ ] Partition runnable work by ecosystem and run one sequential lane per ecosystem.
- [ ] Preserve dependency order inside each ecosystem lane.
- [ ] Preserve stable `PackagePublishReport.packages` ordering by original plan order, not completion order.
- [ ] Stop scheduling new work after failure, while allowing in-flight ecosystem lanes to finish and report outcomes.
- [ ] Add tests for independent npm/cargo concurrency, cross-ecosystem dependencies, failure behavior, and stable report ordering.
- [ ] Move this plan to `docs/plans/completed/` when the third PR lands.

## Decisions

- Use portable Unicode symbols where supported and ASCII fallbacks otherwise.
- stderr is the progress channel so stdout remains usable for results and command composition.
- Captured and CI progress uses complete deterministic lines; animation is interactive-terminal-only.
- Sequential publish reports keep fail-fast execution but represent every expected package, including packages skipped after failure.
- Parallelism starts at ecosystem granularity, not package granularity, to limit registry-rate and dependency-order risk.

## Open question

- Whether parallel publishing should be opt-in for one release before becoming the default.
