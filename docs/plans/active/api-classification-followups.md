# API classification followups

## Goal

Work through the five immediate followups from the API snapshot classification MVP:

1. Dogfood `mc change classify --format markdown` and `mc api diff --format json` on monochange itself.
2. Add CI/docs guidance for advisory `mc changeset validate --api` usage.
3. Improve JavaScript/TypeScript classifier precision so implementation-only exported function body changes are not reported as breaking API changes.
4. Add dependency propagation modes beyond MVP `none`, starting with public dependency propagation.
5. Extend monochange-owned API snapshot support to the next ecosystem.

## Constraints

- Keep work in this isolated worktree on `feat/api-classification-followups`.
- Start with tests for behavior changes.
- Preserve patch coverage at 100%.
- Use `devenv shell ...` for repo validation.
- Do not publish, tag, release, or merge protected branches directly.

## Implementation plan

- [x] Dogfood the merged API classification commands against this branch/main and capture any usability/doc gaps.
- [x] Add a changeset for the followup behavior.
- [x] Document agent/CI advisory usage for:
  - `mc change classify --format markdown`
  - `mc api diff --format json`
  - `mc changeset validate --api --format markdown`
- [x] Add failing tests showing JS and TS exported function body-only changes classify as `patch`/no public API change rather than `major`.
- [x] Update ECMAScript export signature extraction to compare public signatures instead of full declaration bodies.
- [x] Add classification dependency propagation primitives:
  - policy enum (`none`, `public`, with future-proofing for other modes)
  - direct dependents of packages with public API impact get propagated patch recommendations in public mode
  - report output explains propagated impact source.
- [x] Wire dependency propagation into CLI options with a safe default of `none` and docs for `--dependency-propagation public`.
- [x] Add API snapshot extraction support for Dart using the existing Dart public symbol extractor.
- [x] Extend integration tests/fixtures to cover Dart API classification and dependency propagation.
- [x] Rebase on latest `main` and enforce affected-changeset bump alignment against API classification recommendations.
- [x] Run formatting, focused tests, lint/clippy, docs checks, and patch coverage.
- [ ] Open PR, monitor checks, fix failures, then merge when green.

## Notes

- Dogfooding found one CLI usability gap: in this workspace `cargo run -p monochange` must specify `--bin mc` because the package has both `mc` and `monochange` binaries. The docs/skill examples continue to use the installed `mc` binary.
- `coverage:patch` ran the changed suites and API classification coverage successfully, then stopped in an unrelated `group_release_note_fallback` fixture because local git inherited signed-commit config but `gpg` was not available in the coverage environment.
- Public dependency propagation in this followup means direct package-level propagation from a package with public API changes to packages declaring that package as a dependency. It is intentionally conservative and reports propagated recommendations separately from first-party API diffs.
- Dart is the lowest-risk next ecosystem because `monochange_dart` already extracts public Dart symbols for semantic analysis.
