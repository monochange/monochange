# Fix mutation-testing workflow

## Problem statement

The nightly `mutation-testing` workflow has not completed successfully. Recent runs all fail before producing useful mutation reports because each matrix job passes `--output mutants-report/<crate>` without creating the `mutants-report` parent directory first. `cargo-mutants` v27 creates the final output directory but still errors when the parent is missing.

## Goals

- Make the scheduled workflow green for a useful, mutation-clean canary set.
- Keep manual dispatch useful for one crate or the full workspace.
- Preserve report uploads and summarize caught/missed/timeout/unviable counts.
- Avoid forcing the nightly job to run every workspace crate while known equivalent/surviving mutants remain in less-ready crates.

## Plan

- [x] Create an isolated worktree and branch.
- [x] Inspect recent workflow runs and identify the common failure point.
- [x] Reproduce a canary mutation run locally with the required output parent directory.
- [x] Rewrite the workflow matrix discovery around a default canary set plus manual `all`/single-crate modes.
- [x] Ensure the report parent directory exists before invoking `cargo-mutants`.
- [x] Keep `cargo-mutants` running long enough to upload artifacts and then fail intentionally on missed/timeout mutants.
- [x] Add a GitHub step summary with actionable report links/counts.
- [x] Validate workflow syntax and run targeted local checks.
- [ ] Commit, push, open a PR, and ask for a manual workflow run if needed.

## Findings

Recent failed scheduled runs (`26704956853`, `26675892726`, `26621000045`, `26557571380`, `26493969885`) show all matrix jobs failing in the same way:

```text
Error: create output parent directory "mutants-report/<crate>"
Caused by:
    No such file or directory (os error 2)
```

The follow-up report step then fails with `No mutants report found`, so the workflow has been red without surfacing surviving mutants.

A local canary check for `monochange_semver` succeeds once the parent directory exists:

```text
19 mutants tested: 15 caught, 4 unviable, 0 missed
```

Validation completed:

- `devenv shell cargo mutants -p monochange_semver --no-shuffle --output mutants-report/monochange_semver --timeout 120 --minimum-test-timeout 60`
- `devenv shell dprint check .github/workflows/mutation-testing.yml docs/plans/active/fix-mutation-testing-workflow.md`
- `devenv shell zizmor .github/workflows/mutation-testing.yml`
- YAML parse smoke test with Ruby's YAML loader
- Local shell smoke tests for canary/all matrix discovery and report summarization
- `devenv shell mc step:validate`
