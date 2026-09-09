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

### Grounded architecture

The caller remains `monochange change classify --detection-level semantic`; callers do not coordinate declaration emission, entrypoint discovery, or TypeScript processes. `monochange_analysis` supplies immutable package snapshots to the npm adapter, the npm adapter owns TypeScript-specific policy, and the classifier only consumes shared semantic assessments. This preserves the existing boundary: core defines evidence, adapters interpret ecosystems, and the CLI orchestrates.

The existing `SemanticChange` shape can describe syntax differences, but it cannot represent a compiler-proven compatible change or a deliberately inconclusive result. Unit 2 therefore adds an optional assessment with these orthogonal facts:

- compatibility outcome (`breaking`, `additive`, `compatible`, or `unknown`)
- proposed bump (`major`, `minor`, `patch`, or `none`)
- confidence
- engine name and version
- coverage completeness and note
- fallback reason, when the compiler could not complete the check

Old analyzers may omit the assessment and retain the classifier's current conservative defaults. TypeScript findings populate it. This contract is also the integration point for the Rust matrix in Unit 3.

### Usage sketch

Projects install TypeScript in the workspace and opt into the semantic tier:

```bash
pnpm add --save-dev typescript
monochange change classify --detection-level semantic --format json
```

The same command remains usable without Node or TypeScript. In that case, syntax findings remain available, the report records the unavailable engine and fallback reason, and `reviewRequired` stays true.

### Design synthesis

Two shapes were considered:

1. A deep npm-adapter module runs one embedded JavaScript helper, emits declarations from isolated in-memory package snapshots, resolves every explicit typed export, and returns assessed semantic changes. The public surface is one analyzer call and the module hides process control, compiler diagnostics, entrypoint rules, and assignability direction.
2. A generic external-analyzer framework materializes whole repositories and exposes process stages through core. This could support future ecosystems, but it makes callers understand staging and tool lifecycles before a second implementation proves those abstractions.

The first shape is the base. The shared assessment type is the only part generalized for Unit 3. Whole-repository materialization is rejected for this unit because it broadens the trusted and performance-sensitive surface; package-local snapshots remain deterministic, while inherited configs and external dependency state are reported as partial coverage instead of being silently treated as exact historical evidence.

The TypeScript comparison uses the compiler's public `TypeChecker.isTypeAssignableTo` API. New value exports must remain assignable to their old value contracts. Type declarations are checked in both directions because consumers can use exported types as inputs or outputs. Exact declaration equality short-circuits identity-sensitive constructs; changed generic or nominal declarations become inconclusive unless the checker can prove a directional incompatibility without relying on declaration identity. This deliberately favors an honest review requirement over a false major.

Tradeoffs:

- We accept an installed Node and TypeScript requirement for complete semantic evidence in exchange for using the project's real compiler semantics.
- We accept partial results for inherited build configuration and identity-sensitive types in exchange for avoiding false certainty.
- We accept a bounded external process in semantic mode in exchange for keeping basic and signature modes fast and dependency-free.

The first implementation step is a failing npm-adapter fixture that expects declaration-equivalent, additive, breaking, and unavailable-engine outcomes through the shared assessment contract.

- [x] Define analyzer evidence that records engine, version, coverage, outcome, and fallback reason.
- [x] Resolve public TypeScript entry points from package metadata and exports.
- [x] Produce declaration snapshots in isolated before/after trees.
- [x] Ask TypeScript to check consumer-visible assignability in both directions.
- [x] Distinguish breaking, additive, compatible implementation-only, and inconclusive changes.
- [x] Fall back to syntax analysis without overstating confidence when Node, TypeScript, dependencies, or build configuration are unavailable.
- [x] Cover overloads, generics, union narrowing/widening, optionality, enums, classes, re-exports, and declaration generation failures with fixtures.
- [x] Document exact setup and failure modes for agents and CI.

Implemented fixture coverage also includes declaration-only and source entrypoints, sole-entrypoint and optional-member removal, import/require conditional surfaces, readonly transitions, optional arguments, inherited configuration, transitive private types, wildcard exports, JavaScript packages with unpublished TypeScript declarations, root packages, unsafe paths, missing Node, and bounded process input, output, and runtime. The report hashes the complete assessment into finding identity and clamps a supplied bump to the minimum severity implied by its compatibility outcome.

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

- [x] Keep comparison endpoints explicit in human and JSON output.
- [x] Keep analyzer provenance and uncertainty machine-readable.
- [x] Never downgrade a proven breaking change because another analyzer is incomplete.
- [x] Never convert missing evidence into a high-confidence recommendation.
- [x] Keep fixtures file-based and integration snapshots readable.
- [x] Run `devenv shell fix:all`, `devenv shell build:all`, `devenv shell lint:all`, `devenv shell test:all`, `devenv shell coverage:all`, `devenv shell coverage:patch`, and `devenv shell monochange step validate` before queueing each pull request.
