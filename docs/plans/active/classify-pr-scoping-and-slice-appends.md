# Scope classification to the pull request and treat slice appends as additive

Status: in progress.

Issue: [#706](https://github.com/monochange/monochange/issues/706).

## Problem

`monochange change classify` reports two wrong answers for a pull request that does not touch a package, and for a purely additive change to a public slice.

### 1. Default-branch findings set the pull request proposal

A package with no pull request change but with unreleased work on the default branch is attributed that work:

- `crates/monochange_classification/src/classification.rs` builds `comparisons` and `evidence` for every selected package from five frames. The `release` and `releaseToDefault` frames describe what the default branch accumulated since the latest release, not what the pull request contributes.
- `build_recommendation` derives `proposed_changeset_bump` from findings whose `comparisons` contain `ComparisonKind::PullRequest`. A package the pull request never touches has no pull request analysis at all, so it falls through the override at `classification.rs:728-732`: no changed files plus a pending changeset forces `completeness: Unsupported` and `review_required: true`.
- The rendered summary becomes "pending changeset intent has no matching package change and requires review", and `release_floor` stays `major` from the merged work.

The reproduction is `fixtures/tests/api-classification/pr-scoped-release-floor`: `core` is tagged, `main` renames a public reexport, and the pull request only adds an element to a slice in a different package while declaring `core: feat`. `core` reports `proposed=none`, `review_required=true`, `release_floor=major`.

### 2. Slice appends are reported as breaking modifications

`crates/monochange_cargo/src/analysis.rs` renders each public item with `render_signature`, which serializes the whole `syn` item including the initializer expression. `diff_public_symbols` compares those strings, so appending one element to `pub const LINT_NAMES: &[&str] = &[...]` changes the string and produces `SemanticChangeKind::Modified`. With no assessment attached, `monochange_semver::semantic_change_severity` maps `(PublicApi, Modified)` to `major`, and the finding is escalated to `major` even though `cargo semver-checks` reports "no semver update required".

## Scope

- Stop `release` and `releaseToDefault` findings from driving the proposed changeset bump for a package the pull request does not modify.
- Report a package the pull request does not touch as unaffected instead of `reviewRequired`, while keeping the accumulated default-branch evidence in the release floor.
- Classify a pure append to a public `static`/`const` slice literal as additive.
- Keep the report contract versioned; regenerate schema assets and snapshots.
- Update reference docs, the packaged skill, fixtures, and changesets.

## Non-goals

- Modeling every Rust collection mutation (removals, reordering, element edits stay conservative).
- Changing cargo-semver-checks integration.
- Changing release planning, publish readiness, or the release PR body.

## Design

### Pull request scoping

`build_recommendation` already separates pull request findings from release findings. The missing distinction is whether the pull request touched the package at all. `has_current_changes` currently means "the pull request comparison has changed files", but the release/releaseToDefault frames also contribute findings, and `pull_request_changed_files` is the authoritative signal.

- `current` findings must be findings that the pull request actually observed. A finding whose only comparison membership is `release`/`releaseToDefault` describes the default branch. A finding seen in both `pullRequest` and `release` is a pull request finding: it is present in the interval the pull request extends.
- When the pull request touches no file in the package, the package is unaffected: `proposed_changeset_bump` is `none`, `review_required` is false, and the summary says the package is unaffected. `release_floor` and `release_impact` keep reporting the accumulated default-branch interval so the release PR still sees the floor.
- A pending changeset on an untouched package is a deliberate declaration for a consumer-facing effect implemented elsewhere, so the existing `ChangesetAction::Review` behavior stays, without the false `reviewRequired` verdict on the bump.

### Slice appends

Keep `render_signature` for the stored signature (it is the report evidence) and add a structural check in the Cargo analyzer:

- Record the parsed initializer element sequence for `const`/`static` items whose declared type is a slice (`&[T]`, `[T; N]`, `Vec<T>`) and whose expression is an array literal.
- When the before and after element sequences differ, the change is additive if the after sequence starts with the before sequence (a pure append) or the before sequence starts with the after sequence with the same element type. Appends carry an explicit `SemanticChangeAssessment` with `outcome: Additive`, `suggested_bump: Minor`, `confidence: High`, and a coverage note naming the append rule. Removals, reorders, and element edits keep the conservative `major` mapping.
- The assessment flows through `monochange_semver::semantic_change_severity` and `compatibility_impact_from_outcome`, so no classification-side change is needed for this defect.

## Affected files

- `crates/monochange_classification/src/classification.rs`
- `crates/monochange_cargo/src/analysis.rs`
- `crates/monochange_classification/src/__tests__/classification_tests.rs`
- `crates/monochange_cargo/src/__tests__/analysis_tests.rs`
- `crates/monochange_integration_tests/tests/api_classification.rs`
- `fixtures/tests/api-classification/pr-scoped-release-floor/**` (new)
- `fixtures/tests/api-classification/slice-append/**` (new)
- Schema assets under `crates/monochange_classification/schemas/` and `docs/src/schemas/`
- `docs/src/reference/change-classification.md`, `packages/monochange__skill/skills/change-classification.md`

## Validation

- `devenv shell -- cargo test -p monochange_classification --all-features`
- `devenv shell -- cargo test -p monochange_cargo`
- `devenv shell -- cargo test -p monochange_integration_tests --test api_classification`
- `devenv shell fix:all`, `devenv shell lint:all`
- `devenv shell -- monochange step validate`
- `devenv shell coverage:patch` at 100%

## Risks

- A package that the pull request does not touch but whose dependency chain changes must still be reachable through `--dependency-propagation public`; that path creates its own pull request finding and keeps its existing behavior.
- The append rule must not fire when the element type changed, when the declared type is not a slice, or when the change is not a pure suffix. Unmodeled cases stay conservative.
