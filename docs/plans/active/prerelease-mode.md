# First-class prerelease mode

## Status

In progress on branch `feat/prerelease-mode` in the separate worktree:

`/Users/ifiokjr/.pi/agent/worktrees/root/root/Users/ifiokjr/Developer/projects/monochange/monochange/worktrees/chore-prerelease-support-review`

## Problem statement

monochange supports explicit SemVer prerelease versions through changesets, but it does not have a first-class prerelease workflow. Users need a way to repeatedly prepare tagged prerelease builds that write prerelease versions into manifests/versioned files, generate downloadable release artifacts, and avoid publishing packages, while preserving the same changesets for the eventual stable release.

The main correctness risk is repeated prerelease preparation. If a major changeset moves `1.0.0` to `2.0.0-alpha.0`, the next prerelease must become `2.0.0-alpha.1`, not `3.0.0-alpha.0`. Prerelease mode therefore needs to track the original stable version used as the planning base while the prerelease series is active.

## Decisions

- Prerelease preparation updates manifests/versioned files by default.
- The default prerelease base strategy is `planned`: compute the next stable base from changesets and dependency propagation, then append the prerelease suffix.
- Alternate base strategies:
  - `current-stable`: ignore changeset bump severity for the prerelease base and use the original stable version.
  - `fixed`: use a configured `base_version`, such as `0.0.0`.
- Prerelease state is persisted under `.monochange/` so repeated prerelease preparation uses the original stable version instead of the current prerelease manifest version.
- Prerelease mode preserves changesets by default.
- Prerelease mode skips changelog file updates by default.
- Prerelease mode keeps release notes enabled by default so hosted prerelease pages can describe the changes.
- Prerelease mode does not publish packages by default.
- Prerelease mode supports preparing versions even when no changesets exist; in that case it synthesizes prerelease release decisions from discovered packages/version groups and the configured base strategy.
- When prerelease mode is disabled, a leftover `.monochange/prerelease-state.json` is invalid repository state and must fail validation/check/lint instead of being silently ignored.
- A final stable release removes prerelease state after successfully preparing the stable release.

## Proposed configuration

```toml
[prerelease]
enabled = true
channel = "alpha"
numbering = "increment" # increment | date | datetime
base = "planned"        # planned | current-stable | fixed
base_version = "0.0.0"  # required only when base = "fixed"

write_manifests = true
keep_changesets = true
changelog = false
release_notes = true
publish_packages = false
```

## Proposed state file

Path:

```text
.monochange/prerelease-state.json
```

Shape:

```json
{
  "schema_version": 1,
  "channel": "alpha",
  "numbering": "increment",
  "base": "planned",
  "created_at": "2026-05-22T00:00:00Z",
  "updated_at": "2026-05-22T00:00:00Z",
  "packages": {
    "cargo:monochange": {
      "original_stable_version": "1.0.0",
      "planned_stable_version": "2.0.0",
      "latest_prerelease_version": "2.0.0-alpha.1"
    }
  },
  "groups": {
    "rust": {
      "original_stable_version": "1.0.0",
      "planned_stable_version": "2.0.0",
      "latest_prerelease_version": "2.0.0-alpha.1"
    }
  }
}
```

The required invariant is:

```text
active prerelease planning base = state original_stable_version
not current manifest version
```

## Affected areas

- `crates/monochange_core/src/lib.rs`
  - Add public prerelease configuration types.
  - Add prerelease config to `WorkspaceConfiguration`.
- `crates/monochange_config/src/lib.rs`
  - Parse `[prerelease]` from `monochange.toml`.
  - Validate compatible option combinations.
- `crates/monochange/src/workspace_ops.rs`
  - Load/write/delete prerelease state.
  - Override planning baselines while prerelease state is active.
  - Apply prerelease versions before manifest/versioned file updates.
  - Preserve changesets and skip changelogs according to prerelease settings.
  - Delete prerelease state on stable release.
- `crates/monochange/src/release_artifacts.rs`
  - Ensure hosted releases can be marked prerelease and release notes remain available.
- Tests in crate-local `src/__tests__/` directories and integration tests in `crates/monochange_integration_tests` as needed.
- Documentation and examples for `[prerelease]` configuration.

## Implementation checklist

### Planning and configuration

- [x] Create this plan document.
- [x] Start prerelease configuration types in `monochange_core`.
- [x] Finish `PrereleaseConfiguration` integration into `WorkspaceConfiguration` defaults/builders/tests.
- [x] Add `RawWorkspaceConfiguration.prerelease` parsing in `monochange_config`.
- [x] Validate that `base_version` is present only/when needed for `base = "fixed"`.
- [x] Add config parsing tests and snapshots.

### State model

- [x] Add a prerelease state type and serializer/deserializer.
- [x] Store state at `.monochange/prerelease-state.json`.
- [x] Track package and version-group original stable versions.
- [x] Track latest prerelease versions for increment numbering.
- [x] Add tests for missing, malformed, stale, and valid state.

### Version planning

- [x] Inject stored original stable versions as the planning baseline while prerelease mode is active.
- [x] Ensure repeated `planned` prereleases do not repeatedly apply the same bump.
- [x] Implement `current-stable` base strategy.
- [x] Implement `fixed` base strategy.
- [x] Implement channel normalization/validation for SemVer prerelease identifiers.
- [ ] Implement numbering strategies:
  - [x] `increment`
  - [x] `date`
  - [x] `datetime`
- [ ] Reset increment sequence when package/group, stable base, or channel changes.
- [x] Support prerelease planning when no changesets exist by synthesizing release decisions from discovered packages/version groups and the configured base strategy.
- [x] Ensure no-changeset prerelease `planned` mode uses current/original stable as the base instead of failing for an empty release plan.

### Release preparation behavior

- [x] Write prerelease versions into manifests/versioned files when `write_manifests = true`.
- [x] Preserve changesets by default when prerelease mode is enabled.
- [x] Skip changelog file updates by default when prerelease mode is enabled.
- [ ] Keep hosted release notes available when `release_notes = true`.
- [x] Prevent package publishing by default when prerelease mode is enabled.
- [ ] Mark source-host releases as prereleases when supported.
- [x] Delete prerelease state during the final stable release after successful preparation.
- [x] Fail validation/check/lint when prerelease mode is disabled and `.monochange/prerelease-state.json` still exists.

### Tests

- [x] Unit tests for config defaults and parsing.
- [x] Unit tests for prerelease state read/write/delete. (Read coverage plus integration write/delete coverage.)
- [x] Unit tests for repeated major/minor/patch prerelease planning.
- [x] Unit tests for `current-stable` and `fixed` base strategies.
- [x] Unit tests for channel/numbering formatting.
- [x] Integration tests using file fixtures and Insta snapshots in `crates/monochange_integration_tests`, matching the repository's current integration-test format.
- [x] Integration tests for no-changeset prerelease preparation with `planned`, `current-stable`, and `fixed` base strategies.
- [x] Integration tests for repeated no-changeset prereleases incrementing from existing JSON state.
- [x] Integration tests for stale `.monochange/prerelease-state.json` failing validation/check/lint when prerelease mode is disabled.
- [x] Integration tests for stable release cleanup deleting `.monochange/prerelease-state.json`.
- [x] Use Insta snapshots for observable command output/file-update output; redact timestamps, temp paths, generated IDs, and other unstable values.
- [x] Keep JSON snapshot readability: do not snapshot multiline JSON fields with escaped newlines; redact multiline JSON fields and snapshot multiline content separately when needed.
- [x] Patch coverage remains 100% for executable changed lines.

### Documentation

- [x] Document `[prerelease]` in the configuration docs.
- [x] Document the state-file behavior and stable-release cleanup.
- [ ] Add examples for alpha, rc, date, datetime, fixed `0.0.0`, and no-publish binary-release workflows.

### Validation and PR

- [x] Run formatting/fix command. (`cargo fmt`; `fix:all` pending.)
- [x] Run build command. (`cargo build --workspace --all-features` passes.)
- [x] Run lint/typecheck command. (`cargo clippy --workspace --all-features --all-targets -- -D warnings`, `mc step:validate`, and `mc check` pass.)
- [x] Run tests. (`cargo test -q` passes after updating current and versioned schema assets, clippy fixes, and prerelease-state field rename.)
- [x] Run patch coverage and reach 100%. (`350/350 (100%)` after focused tests and narrow ignores.)
- [ ] Commit signed changes on `feat/prerelease-mode`.
- [ ] Push branch.
- [ ] Create pull request.
- [ ] Monitor pull request checks.
- [ ] Fix any CI/check failures.
- [ ] Notify maintainer when the PR is ready for review.

## Validation commands

Use the repository devenv scripts from the separate worktree:

```nu
devenv shell fix:all
devenv shell build:all
devenv shell lint:all
devenv shell coverage:patch
```

If local devenv linking fails because of the known macOS SDK/libiconv issue, record the failure in this plan and rely on CI after pushing the PR, while still running any available prebuilt `target/release/mc` checks that do not require relinking.

## Progress log

- 2026-05-22: Created implementation branch `feat/prerelease-mode` in the separate worktree.
- 2026-05-22: Started adding prerelease configuration types to `crates/monochange_core/src/lib.rs`.
- 2026-05-22: Added this plan to `docs/plans/active/prerelease-mode.md`.
- 2026-05-22: Updated prerelease state format decision from TOML to JSON at `.monochange/prerelease-state.json`.
- 2026-05-22: Added requirements for no-changeset prerelease support, stale state validation failures when prerelease mode is disabled, and fixture-based Insta integration tests with stable redactions and 100% patch coverage.
- 2026-05-22: Implemented no-changeset prerelease plan synthesis and stale prerelease-state validation failure; added focused unit tests and confirmed `cargo check -q` plus filtered prerelease/config tests pass.
- 2026-05-22: Added fixture-based Insta integration tests for planned no-changeset prerelease preparation and stale prerelease state check failure; ran `cargo fmt`, `cargo check -q`, focused prerelease/config tests, and `cargo test -q -p monochange_integration_tests --test release_record_artifacts -- --nocapture`.
- 2026-05-22: Expanded the no-changeset integration snapshot to cover `planned`, `current-stable`, and `fixed` base strategies, then reran `cargo test -q -p monochange_integration_tests --test release_record_artifacts -- --nocapture`.
- 2026-05-22: Added a repeated no-changeset prerelease integration test proving the second committed prerelease run advances from `alpha.0` to `alpha.1` using `.monochange/prerelease-state.json`; reran `cargo test -q -p monochange_integration_tests --test release_record_artifacts -- --nocapture`.
- 2026-05-22: Added a stable-release cleanup integration test proving a final stable prepare removes `.monochange/prerelease-state.json`; reran `cargo test -q -p monochange_integration_tests --test release_record_artifacts -- --nocapture` with 5 passing tests.
- 2026-05-22: Added prerelease configuration parsing/default unit coverage with an Insta JSON snapshot; reran `cargo test -q -p monochange_config prerelease -- --nocapture` with 3 passing tests.
- 2026-05-22: Added prerelease state unit coverage for missing, valid, and malformed JSON state; reran `cargo test -q -p monochange prerelease_ -- --nocapture` with 5 passing tests.
- 2026-05-22: Documented `[prerelease]`, base strategies, no-changeset synthesis, JSON state, stale-state validation, and stable cleanup in `docs/src/guide/04-configuration.md`; reran `cargo check -q`, focused config tests, focused prerelease unit tests, and release artifact integration tests.
- 2026-05-22: Ran `cargo test -q`; all completed crates passed until `monochange --test cli_progress` exposed an expected progress snapshot change for the new `load prerelease state` phase. Updated the snapshot and reran `cargo test -q -p monochange --test cli_progress -- --nocapture` successfully.
- 2026-05-22: Reran `cargo test -q`; all prior suites passed and the run reached `xtask`, where `committed_schema_modes_are_up_to_date` failed because generated schemas needed the new `[prerelease]` fields. Ran `cargo run -q -p xtask -- schema update`; focused xtask schema test now passes.
- 2026-05-22: Reran `cargo test -q`; the run then failed `schema_assets::versioned_schema_assets_use_stable_ids_without_changing_contracts` because the immutable current-version schema assets also needed the prerelease config fields. Ran `cargo run -q -p xtask -- schema release update --versioned`; focused schema-assets test now passes.
- 2026-05-22: Reran full `cargo test -q`; all workspace tests and doctests pass.
- 2026-05-22: Ran `cargo fmt --check` and `cargo check -q`; both pass.
- 2026-05-22: Ran `cargo clippy --workspace --all-features --all-targets -- -D warnings`; fixed `PrereleaseConfiguration` bool-heavy config lint consistently with existing config structs, replaced generic prerelease `Default::default()` uses in test fixtures, renamed internal prerelease state struct fields while preserving JSON keys, and reran clippy successfully.
- 2026-05-22: Reran full `cargo test -q` after clippy fixes; all workspace tests and doctests pass.
- 2026-05-22: Generated coverage data; overall line coverage is 96.03%, and initial patch coverage command reported `0/0 (100%)` because the branch has not been committed yet. Added `.changeset/prerelease-mode.md`; will rerun patch coverage after committing so `HEAD` contains the patch.
- 2026-05-22: Patch coverage after the first commit was 260/367 (70.84%). Added focused unit coverage for invalid prerelease config branches, prerelease numbering/base mismatch branches, grouped no-changeset prerelease state creation, and unsupported prerelease state schema handling.
- 2026-05-22: Expanded focused prerelease/config tests for read failures, invalid channels, stale-state validation, fixed-base overrides, unplanned groups, and skipped stale/unknown state entries. Added narrow patch-coverage ignores for practically unreachable serialization and OS deletion race branches. Final patch coverage is 350/350 (100%).
- 2026-05-22: Reran `cargo fmt --check`, `cargo llvm-cov --workspace --all-features --lcov --output-path target/coverage/lcov.info`, `pnpm node scripts/check-patch-coverage.mjs --repo-root $PWD --lcov target/coverage/lcov.info --base origin/main --head HEAD --target 100`, `cargo clippy --workspace --all-features --all-targets -- -D warnings`, `cargo build --workspace --all-features`, `cargo run -q -p monochange --bin mc -- step:validate`, and `cargo run -q -p monochange --bin mc -- check`; all pass.
