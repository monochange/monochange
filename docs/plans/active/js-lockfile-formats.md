# JavaScript lockfile formats: pnpm 12, Yarn, and Bun

## Problem

- pnpm 12 writes `pnpm-lock.yaml` as two YAML documents (env lockfile with `configDependencies` plus the project lockfile). `update_pnpm_lock_text` validated with a single-document parser, so releases in pnpm 12 workspaces failed with `failed to parse pnpm lock yaml` and required a manual `pnpm install --lockfile-only` refresh.
- Yarn lockfiles (`yarn.lock`, Classic and Berry) were not supported at all: no kind variant, no updater, no discovery, no config allowlist.
- `update_bun_lock` replaced the first `"name": "..."` string it found, which corrupts real Bun JSONC lockfiles (for example overwriting `"ui": "workspace:packages/ui"` with the released version).
- `load_configured_npm_package` anchored configured packages to their manifest directory, so workspace-root lockfiles were never inferred during release preparation and manager detection missed `bun.lock` and `yarn.lock`.

## Scope

- pnpm: validate every YAML document and scan all top-level section occurrences so pins rewrite in the project document while the env document, separators, quoting, and formatting stay untouched.
- Yarn: `yarn.lock` support for Classic (`version "1.2.3"`) and Berry (`version: 1.2.3`, `0.0.0-use.local` workspace placeholders) with a formatting-preserving updater; `yarn install --mode=update-lockfile` default command; discovery, config allowlist, readiness fingerprinting (already present), and npm-manager detection.
- Bun: restrict rewrites to exact semver pins and `name@version` package-map descriptors; leave `workspace:`, alias, protocol, range, and URL references and JSONC formatting alone.
- npm-family workspace-root resolution for configured packages, mirroring the cargo adapter's walk-up.

## Non-goals

- Rewriting Yarn/Bun entry keys, resolutions, or checksums; configure `lockfile_commands` when package-manager resolution is required.
- Decompressing or restructuring binary `bun.lockb` beyond the existing byte-splice updater.

## Affected files

- `crates/monochange_npm/src/lib.rs` — kind enum, dispatch, updaters, validation, workspace-root walk-up.
- `crates/monochange/src/versioned_files.rs` — `YarnLock` read and apply arms.
- `crates/monochange_config/src/lib.rs` — `yarn.lock` in `path_is_supported_for_ecosystem`.
- Fixtures under `fixtures/tests/npm/` and `fixtures/npm/workspace-yarn/`.
- Docs: `docs/src/guide/ecosystems.md`, `.templates/crates.t.md` (mdt provider), readme consumers.

## Validation

- `cargo test -p monochange_npm --lib tests::lib_tests`
- `cargo test -p monochange --lib`
- `cargo test -p monochange_integration_tests --test npm_pnpm12_two_document_lock --test npm_yarn_locks --test npm_bun_text_lockfile`
- `monochange step validate` and `monochange step prepare-release --dry-run --format json`
- `dprint fmt`, `cargo clippy --all-targets`, `mdt check`
- `coverage:patch` at 100%

## Status

Implementation complete; validation in progress. Follow-up risk: Yarn and Bun direct rewrites intentionally leave entry keys and checksums to a configured lockfile command, matching the existing pnpm behavior.
