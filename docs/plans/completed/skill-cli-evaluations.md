# Extensive CLI and skill evaluations

## Scope

Expand the existing 29 scenarios to 60 realistic adoption, configuration, planning, and recovery scenarios. Compare a frozen shipping skill, a concise candidate, and the revised package skill using isolated agents and the built CLI. Grade observable artifacts and retain raw evidence. Local previews and fixture mutations are permitted; publishing, release PR merges, tags, and releases are outside this work.

## Work

- [x] Audit the runner for false passes, isolation, replay, and selection bugs; add execution/provenance safeguards and harness tests.
- [x] Add 31 scenarios across six ecosystems and different configuration choices (60 total: 38 agent tasks and 22 contracts).
- [x] Preserve the current shipping skill and create a complete concise alternative.
- [x] Execute initial CLI probes and agent cohorts against a preserved CLI snapshot; retain reports and transcripts.
- [x] Finish fresh regrading and repeat divergent scenario/variant cells before selecting guidance.
- [x] Correct reproduced bugs and stale guidance with focused source tests/fixtures.
- [x] Verify all corrections against the final build; the JSON edit fix postdates the frozen cohort binary.
- [x] Record per-scenario outcomes and limitations without claiming absence of bugs.
- [x] Run final formatting, lint, documentation sync, tests/contracts, and 100% patch coverage.
- [x] Prepare signed commits for a PR stack; required CI remains the merge gate.

## Evidence

The suite lives in `evals/monochange-skill/`. Raw transcripts and scratch workspaces remain local. `FINDINGS.md` records comparison results and defects; `COVERAGE.md` maps the scenario inventory to the supported surfaces.
