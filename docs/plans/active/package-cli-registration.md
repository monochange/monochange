# Package CLI registration and command-surface classification

## Status

- Date: 2026-09-12
- Scope: config schema, CLI snapshot capture, change classification, docs, and skill
- Outcome: shipped — packages register a CLI, snapshots feed classification
- Code changes: yes (see PR notes below)

## Problem

The breaking-change comment on pull requests could not recognize that a package ships a CLI. The cargo/npm analyzers model library public API only, so command-surface changes in a CLI's argument definitions surfaced as `monochange/unclassified-source` findings with `impact: unknown`. The building blocks existed — `monochange_snapshot` models a normalized `CommandSnapshot`, `diff_command_snapshots` classifies severity, and `change classify` accepted manual `--cli-snapshot-before/after` files — but nothing registered a CLI, captured snapshots automatically, or fed the diff into the classification report.

## Design

- `[package.<id>].cli = { name, snapshot }` registers a CLI. Both fields are required; `snapshot` accepts a command string or a `{ command, cwd, shell }` table mirroring `LockfileCommandDefinition`. Identity-only forms (`cli = true`, `cli = "name"`) were rejected in review: without a capture command they add statuses without findings.
- One CLI per package. The registration is additive to `package_type`, so a package keeps its library/registry surface and gains a command-surface identity.
- The snapshot command must print a `CommandSnapshot` JSON document on stdout; `schema_version` is validated against `SNAPSHOT_SCHEMA_VERSION`.
- Baselines are committed release state under `.monochange/cli-snapshots/<name>.json`, refreshed by an explicit release-workflow step (`monochange snapshot --package <id> --save`). Automatic capture inside prepare-release is deferred because it forces a mid-release build.
- Classification diffs baseline (before) against a fresh capture (after) for every classified package with a registered CLI whose files changed, and appends `monochange/cli-surface` findings: removals/narrowing are breaking (`major`), additions/widening additive (`minor`), description changes compatible. Confidence high, coverage complete, per-command `max_bump` caps honored. The comparison status lands in an additive per-package `cli` block; capture problems degrade to warnings. `--skip-cli-snapshots` and `MONOCHANGE_SKIP_CLI_SNAPSHOTS=1` opt out.
- The classification report schema version bumps to 4.

## Testing

- Config unit tests: string and table `snapshot` forms, unknown-field rejection, empty name/command errors, duplicate CLI name error, field defaulting (`crates/monochange_config/src/__tests__/lib_tests.rs`).
- Mapping unit tests: impact/severity/kind-name tables, finding id uniqueness, coverage fields, status descriptions, skip-flag logic (`crates/monochange/src/__tests__/change_classify_tests.rs`).
- Module unit tests: baseline read/save/staleness, capture success through shell and split-command paths, failure modes (non-zero exit, invalid stdout, schema mismatch, unparseable command, missing program), `--package/--save/--list` behavior (`crates/monochange/src/__tests__/cli_surface_tests.rs`).
- Integration tests with file fixtures (`fixtures/tests/cli-snapshot-registration/`, `crates/monochange_integration_tests/tests/cli_surface_classification.rs`): diffed breaking CLI change raising `enforceableMinimum` to `major`, text and markdown comment rendering, `missing_baseline`, `stale_baseline`, `failed`, `skipped`, snapshot list/save lifecycle, and error paths for unknown/unregistered packages.

## Documentation

- `docs/src/reference/package-cli-registration.md` (new) and a CLI section in `docs/src/reference/change-classification.md`, linked from `SUMMARY.md`.
- `monochange.toml` field documentation block and self-registration for the `monochange` package, including the release-workflow capture step.
- Skill bundle updates in `packages/monochange__skill` (SKILL.md, skills/configuration.md, skills/commands.md, skills/change-classification.md).
- Committed JSON schema assets regenerate with the new definitions.
