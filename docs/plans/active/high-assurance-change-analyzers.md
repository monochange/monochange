# High-assurance change analyzers

## Goal

Make `monochange change classify` dependable enough for an agent to propose a changeset bump from repository evidence, while preserving explicit uncertainty when an ecosystem cannot be checked completely.

The program closes four known gaps:

1. Discover packages that exist only in the comparison baseline because the candidate deleted them completely.
2. Compare the generated TypeScript declaration surface with TypeScript's type system instead of treating source syntax as the public API.
3. Check Rust compatibility across declared feature sets and configured target combinations, using cargo-semver-checks evidence where available.
4. Calibrate recommendations against historical pull requests before making a strict policy the default.

## Delivery strategy

Ship one reviewable pull request per numbered unit. Each unit must keep the existing JSON contract backward compatible unless its plan explicitly versions the schema. Queue and merge a unit only after its fixtures, patch coverage, workspace validation, documentation, and agent instructions pass. Start the next unit from the merged default branch.

## Unit 1: Baseline-only packages

- [x] Add a fixture where a configured package and its public API are removed.
- [x] Discover package metadata from both snapshot endpoints.
- [x] Rebase snapshot-discovered paths to the real repository root and merge packages by ecosystem plus repository-relative manifest path.
- [x] Preserve baseline package configuration and path matching when the candidate removed its `monochange.toml` package entry.
- [x] Analyze the deleted package with a populated before snapshot and an empty after snapshot.
- [x] Recommend `major` with high-confidence removed-package lifecycle evidence and retain partial source-level API details.
- [x] Document fully deleted package behavior for users and agents.

Acceptance: `change classify --base <base> --head <head>` includes the removed package, identifies its exported symbols as removed, and proposes `major`.

## Unit 2: TypeScript declaration compatibility

- [ ] Define analyzer evidence that records engine, version, coverage, outcome, and fallback reason.
- [ ] Resolve public TypeScript entry points from package metadata and exports.
- [ ] Produce declaration snapshots in isolated before/after trees.
- [ ] Ask TypeScript to check consumer-visible assignability in both directions.
- [ ] Distinguish breaking, additive, compatible implementation-only, and inconclusive changes.
- [ ] Fall back to syntax analysis without overstating confidence when Node, TypeScript, dependencies, or build configuration are unavailable.
- [ ] Cover overloads, generics, union narrowing/widening, optionality, enums, classes, re-exports, and declaration generation failures with fixtures.
- [ ] Document exact setup and failure modes for agents and CI.

Acceptance: declaration-equivalent source refactors recommend `none`; additive declarations recommend `minor`; incompatible declarations recommend `major`; unavailable type-system evidence is reported as partial rather than guessed.

## Unit 3: Rust semantic compatibility matrix

- [ ] Define an explicit configuration for feature and target combinations.
- [ ] Run cargo-semver-checks against isolated before/after package snapshots.
- [ ] Record the exact feature/target matrix, tool version, skipped cells, and diagnostics in analyzer evidence.
- [ ] Merge results conservatively: any proven break is `major`; additive API is `minor`; compatible implementation-only changes are `none`; incomplete cells keep the result partial.
- [ ] Preserve the syntax analyzer as a deterministic fallback.
- [ ] Cover default features, no-default-features, all-features, selected features, target-gated APIs, trait changes, and unavailable-tool behavior.
- [ ] Document installation and CI setup without invoking release workflows.

Acceptance: feature- or target-gated breaks cannot disappear behind a successful default build, and every recommendation states which matrix cells were checked.

## Unit 4: Historical calibration and strict rollout

- [ ] Add a checked-in corpus manifest of representative merged changes and the human-authored changeset outcomes used as labels.
- [ ] Replay analyzer inputs deterministically without network access in tests.
- [ ] Report precision, recall, false-major, false-none, and inconclusive rates by ecosystem and evidence level.
- [ ] Define thresholds for advisory, warning, and strict enforcement modes.
- [ ] Keep strict mode opt-in unless the measured corpus meets the documented thresholds and inconclusive results fail safely.
- [ ] Publish the calibration method and limitations for maintainers and agents.

Acceptance: a strict-default decision is backed by reproducible measurements, not fixture success alone.

## Cross-cutting quality bar

- [ ] Keep comparison endpoints explicit in human and JSON output.
- [ ] Keep analyzer provenance and uncertainty machine-readable.
- [ ] Never downgrade a proven breaking change because another analyzer is incomplete.
- [ ] Never convert missing evidence into a high-confidence recommendation.
- [ ] Keep fixtures file-based and integration snapshots readable.
- [ ] Run `devenv shell fix:all`, `devenv shell build:all`, `devenv shell lint:all`, `devenv shell test:all`, `devenv shell coverage:all`, `devenv shell coverage:patch`, and `devenv shell monochange step validate` before queueing each pull request.
