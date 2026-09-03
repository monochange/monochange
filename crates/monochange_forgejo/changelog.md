# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.10.0](https://github.com/monochange/monochange/releases/tag/v0.10.0) (2026-09-03)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.10.0 as part of group `main`.

## [0.9.2](https://github.com/monochange/monochange/releases/tag/v0.9.2) (2026-08-29)

<details>
<summary><strong>📖 Documentation</strong></summary>

#### add the monochange logo across readme, docs.rs, and the mdBook

Every published crate now renders the monochange mark on docs.rs through `html_logo_url`, and docs.rs pages use the matching favicon through `html_favicon_url`. The mark itself is a chunky lowercase `mc` monogram with a version-bump arrow in the negative space.

- the readme gains a top-level hero logo that follows the reader's theme: a light variant on light GitHub themes and a light-on-dark variant on dark themes, using the `picture` element with `prefers-color-scheme`
- the mdBook in `docs/` picks up a new `favicon.png`
- `assets/` holds the exported logo sizes (280, 512, 1024), the dark variant, and a multi-size `favicon.ico`
- a reserve mark (the navy Converge badge) is kept under `assets/reserve/` for a future rebrand

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #641](https://github.com/monochange/monochange/pull/641)

</details>

## [0.9.1](https://github.com/monochange/monochange/releases/tag/v0.9.1) (2026-08-19)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.9.1 as part of group `main`.

## [0.9.0](https://github.com/monochange/monochange/releases/tag/v0.9.0) (2026-08-14)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.9.0 as part of group `main`.

## [0.8.4](https://github.com/monochange/monochange/releases/tag/v0.8.4) (2026-07-11)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.8.4 as part of group `main`.

## [0.8.3](https://github.com/monochange/monochange/releases/tag/v0.8.3) (2026-06-29)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.8.3 as part of group `main`.

## [0.8.2](https://github.com/monochange/monochange/releases/tag/v0.8.2) (2026-06-18)

### 🐛 Fixed

#### Move rate-limit policy planning into publish core

Keep `monochange` as the CLI crate while moving publish rate-limit policy and window planning helpers into `monochange_publish`.

Ecosystem manifest update planning now lives in the relevant ecosystem crates with `monochange` acting as the CLI orchestrator. Hosted-source adapters now own release URL and release request planning behavior. The test helper crate also centralizes binary lookup for integration tests that need the `monochange` executable. `monochange_github` constrains the GitHub client transitive dependency set so release-job lockfile regeneration stays compatible with the pinned nightly toolchain.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #614](https://github.com/monochange/monochange/pull/614)

## [0.8.1](https://github.com/monochange/monochange/releases/tag/v0.8.1) (2026-06-09)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.8.1 as part of group `main`.

## [0.8.0](https://github.com/monochange/monochange/releases/tag/v0.8.0) (2026-06-04)

### 🐛 Fixed

#### Update package documentation for the nested CLI command API

Updated generated package documentation, skill guidance, provider-facing examples, and release-record schema fixture text to refer to the new `monochange step <name>` and `monochange run <name>` command paths where those packages expose or document monochange CLI workflows.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #597](https://github.com/monochange/monochange/pull/597) · _Related issues:_ [#35](https://github.com/monochange/monochange/issues/35)

## [0.7.0](https://github.com/monochange/monochange/releases/tag/v0.7.0) (2026-06-03)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.7.0 as part of group `main`.

## [0.6.8](https://github.com/monochange/monochange/releases/tag/v0.6.8) (2026-05-31)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.8 as part of group `main`.

## [0.6.7](https://github.com/monochange/monochange/releases/tag/v0.6.7) (2026-05-30)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.7 as part of group `main`.

## [0.6.6](https://github.com/monochange/monochange/releases/tag/v0.6.6) (2026-05-29)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.6 as part of group `main`.

## [0.6.5](https://github.com/monochange/monochange/releases/tag/v0.6.5) (2026-05-29)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.5 as part of group `main`.

## [0.6.4](https://github.com/monochange/monochange/releases/tag/v0.6.4) (2026-05-28)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.4 as part of group `main`.

## [0.6.3](https://github.com/monochange/monochange/releases/tag/v0.6.3) (2026-05-28)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.3 as part of group `main`.

## [0.6.2](https://github.com/monochange/monochange/releases/tag/v0.6.2) (2026-05-27)

### 🐛 Fixed

#### Refresh documentation audit coverage

Updates documentation, CLI help text, package README content, and packaged skill guidance so the documented command surface matches the current monochange CLI and release workflow behavior.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #546](https://github.com/monochange/monochange/pull/546)

## [0.6.1](https://github.com/monochange/monochange/releases/tag/v0.6.1) (2026-05-24)

### Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.6.1 as part of group `main`.

## [0.6.0](https://github.com/monochange/monochange/releases/tag/v0.6.0) (2026-05-23)

### 💥 Breaking Change

#### Async migration: Tokio async runtime end-to-end

This is a **breaking change** that migrates the entire CLI and workspace from synchronous I/O to Tokio async. All public APIs that previously returned `Result<T, E>` directly now return `impl Future<Output = Result<T, E>>` and must be `.await`ed.

The migration was made to reduce release-planning latency by overlapping external work, adding cancellation and timeout boundaries around hosted-source requests, and removing repeated manifest discovery from common policy paths. On a 200-package / 500-changeset / 500-commit fixture, the direct step-command benchmark matrix improved across every measured command with `0` regressions. Across the eight-command matrix, wall-clock time dropped by about **45% on average** (geometric mean about **3.0× faster**, arithmetic mean about **8.3× faster** because the fastest policy paths improved dramatically).

Notable wins:

- `mc step:affected-packages --dry-run --format json` improved from `1442.3 ms` to `35.8 ms` — about **40.3× faster** — by using configuration-only package/group indexes for changeset-policy checks instead of paying full manifest discovery cost.
- The explicit no-changeset affected-package path now completes in about `7.7 ms`, roughly **159× faster** than the pre-optimization async implementation.
- `mc step:diagnose-changesets --dry-run --format json` improved from `3072.2 ms` to `184.9 ms` — about **16.6× faster** — by using the same fast config-id path before falling back to discovery.
- `mc step:prepare-release --dry-run --format json` improved from `2374.8 ms` to `858.5 ms` — about **2.8× faster** — while retaining deterministic release output.
- Short command startup stayed fast by using a current-thread Tokio runtime for `mc`, `monochange`, and `xtask`; previously noisy commands such as `step:config` and `step:display-versions` now benchmark faster than `main`.

##### Breaking changes — Public API signatures

###### `monochange_core`

- **`git_command`** now returns `std::process::Command` (unchanged for inspection compatibility), but all execution functions (`git_checkout`, `git_clone`, `git_commit`, `git_push`, `git_fetch`, `git_merge`, `git_default_branch_name`, `git_rebase`, `git_create_branch`, `git_delete_branch`, `git_tag_create`, `git_read_tree`, `git_status`, etc.) are now `async fn` returning `impl Future`. Callers must `.await` these.
- **`DiscoverOptions`**, **`discover_workspace`**, and all git helper functions are now async.

###### `monochange_hosting`

- **All provider trait methods** (`verify_release_branch`, `publish_release`, `retarget_release_tags`, `create_release_pull_request`, `update_release_pull_request`, `find_existing_pull_request`, `find_existing_merge_request`, `find_existing_release`, `enrich_changeset_context`, `default_branch_name`, `no_identity`) are now `async fn`. Implementors must update their trait implementations.
- Provider lookup functions (`get_hosting_provider`, `get_provider`) remain sync.

###### `monochange_github`, `monochange_gitea`, `monochange_gitlab`, `monochange_forgejo`

- **All public sync entry points** that previously used `Runtime::new().block_on()` internally are now `async fn`; the sync bridge helper is kept only for tests. Public `async fn` signatures include:
  - `publish_release`, `find_existing_pull_request`, `find_existing_release`, `default_branch_name`, `create_change_request`, `verify_release_branch`, `retarget_release_tags`, `enrich_changeset_context`, `no_identity`
- **`reqwest::blocking::Client`** replaced with async `reqwest::Client` throughout.
- **`github_runtime()` / `gitea_runtime()` / etc.** removed from public API (only available as `#[cfg(test)]` helpers).

###### `monochange_publish`

- **`filter_pending_publish_requests`**, **`filter_pending_publish_requests_with_transport`**, **`registry_version_exists`**, **`crates_io_version_exists`**, **`crates_io_index_version_exists`** are now `async fn`. Callers must `.await`.
- **`execute_publish_requests`**, **`execute_publish_requests_with_progress`**, **`execute_publish_requests_with_process`**, **`execute_publish_requests_with_process_and_progress`**, and **`run_placeholder_publish`** are now async.
- **`reqwest::blocking::Client`** replaced with async `reqwest::Client` throughout.
- **`registry_client()`** is now a sync function returning `MonochangeResult<Client>` (no longer async).

###### `monochange`

- **`cli_runtime::block_on_in_context`** is now a `#[cfg(test)]` `pub(crate)` helper for compatibility tests; production code awaits async APIs directly.
- **`publish_source_change_request`**, **`publish_readiness::publish_plan_package_filter_from_readiness_artifact`**, **`plan_publish_rate_limits`**, and all async CLI step handlers are now `async fn`.
- **`run_publish_packages`**, **`run_publish_packages_with_resume`** remain async.
- **`run_placeholder_publish`** and **`execute_publish_requests_with_process`** are now async and should be awaited directly.
- **Test files** converted from `#[test]` to `#[tokio::test(flavor = "multi_thread")]` where they call async code.

##### Migration guide

1. Any code calling `monochange_core::git::*` functions must `.await` the result.
2. Any code using `monochange_hosting` provider traits must implement `async fn` methods.
3. Any code calling `monochange_publish::*` async functions must be in an async context.
4. Tests that call async code must use `#[tokio::test(flavor = "multi_thread")]`.
5. Replace `reqwest::blocking::Client` with `reqwest::Client` (async) in all custom code.
6. Avoid new sync-to-async bridges in production code; prefer async callers and `.await`. `block_on_in_context` is retained only for test compatibility boundaries.

```rust
// Before (sync):
let result = monochange_core::git::git_checkout(&repo_dir, branch)?;

// After (async, must await):
let result = monochange_core::git::git_checkout(&repo_dir, branch).await?;

// Before (sync):
let pending = monochange_publish::filter_pending_publish_requests(&config)?;

// After (async, must await):
let pending = monochange_publish::filter_pending_publish_requests(&config).await?;
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #440](https://github.com/monochange/monochange/pull/440) _Introduced in:_ [`10ef5bd`](https://github.com/monochange/monochange/commit/10ef5bda1e30003018408c9a6c1758af69e781aa) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63) _Closed issues:_ [#407](https://github.com/monochange/monochange/issues/407)

### 🐛 Fixed

#### Add dist profile and ring TLS backend for binary size reduction

- Add `[profile.dist]` for optimized CI/release builds (LTO, codegen-units=1, strip)
- Feature-gate `rmcp`/MCP server behind `mcp` feature (default-enabled, ~313 KiB savings when disabled)
- Replace `EnvFilter` with `LevelFilter` in tracing setup (~1.4 MiB savings from removing tracing-log and regex)
- Switch TLS backend from `aws-lc-rs` to `ring` (~2.5 MiB binary size reduction)
- Install ring crypto provider at startup (required for rustls-no-provider)
- Remove `default-features = true` on `reqwest` workspace references (was re-enabling default TLS)
- Wire `dist` profile into binary-size CI job and release workflow
- Remove redundant `CARGO_PROFILE_RELEASE_*` env vars from release workflow
- Add `build:dist` and `test:dist` devenv scripts for dist-profile validation
- Add `test_dist` CI job to run tests against dist-optimized build
- Add `build dist profile` step to CI build job (Linux only)

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #521](https://github.com/monochange/monochange/pull/521) _Introduced in:_ [`a49bbc1`](https://github.com/monochange/monochange/commit/a49bbc1eb8f04699e99de01d159de67c8f48a160) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

#### Add optional full release staging

Release commit and release request steps now support a `stage_all` input/config field that defaults to `false`. When enabled, the release commit stages every non-ignored working tree change, so generated lockfile updates like `pnpm-lock.yaml` can be included alongside configured release manifests and changelogs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #520](https://github.com/monochange/monochange/pull/520) _Introduced in:_ [`035dcb3`](https://github.com/monochange/monochange/commit/035dcb345cca8586440451836fa06fb631596c20) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 📝 Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 🚀 Feature

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### 🚀 Feature

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### 📝 Changed

- No package-specific changes were recorded; `monochange_forgejo` was updated to 0.4.1 as part of group `main`.

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### 🚀 Feature

#### Add Forgejo source provider

Add Forgejo as a hosted source provider for releases and release pull requests.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #401](https://github.com/monochange/monochange/pull/401) _Introduced in:_ [`86026ac`](https://github.com/monochange/monochange/commit/86026acb83e338fe8d07c200fb8e38693616b6e8)

### 🐛 Fixed

#### Remove grouped release member summaries

Grouped release notes no longer include generated changed or synchronized member lists, keeping the release note summary focused on the group release itself.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #395](https://github.com/monochange/monochange/pull/395) _Introduced in:_ [`2d012ff`](https://github.com/monochange/monochange/commit/2d012ff900a612f4aed6e4d7034c8c876f50aeae) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### 🧪 Testing

#### Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #428](https://github.com/monochange/monochange/pull/428) _Introduced in:_ [`b61cc3e`](https://github.com/monochange/monochange/commit/b61cc3e66989fd83ffb16a31568d2f46d7075216)
