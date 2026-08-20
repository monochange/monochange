# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.9.1](https://github.com/monochange/monochange/releases/tag/v0.9.1) (2026-08-19)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.9.1 as part of group `main`.

## [0.9.0](https://github.com/monochange/monochange/releases/tag/v0.9.0) (2026-08-14)

### 🚀 Feature

#### Add a configurable publish timeout with retries and a Dart protected-publishing warning

- New `publish.timeout` settings (`timeout_seconds` default 60, `retries` default 2) cap how long a single package publish command may hang before it is killed and retried. Set `timeout_seconds = 0` to disable the timeout.
- Publish commands that time out are retried up to `retries` times; after the final attempt the package is reported as timed out instead of hanging the whole job.
- Dart/pub.dev packages using protected (trusted) publishing from a GitHub Actions `workflow_dispatch` event without a `PUB_TOKEN` fallback now emit a warning explaining that pub.dev automated publishing may require publishing from a pushed tag rather than a workflow dispatch

Configure the timeout per package or ecosystem:

```toml
[package.my-dart-package.publish.timeout]
timeout_seconds = 90
retries = 3
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #630](https://github.com/monochange/monochange/pull/630)

### 🐛 Fixed

#### Report every expected package after publishing stops on failure

Sequential package publishing remains fail-fast, but its report now includes every package that was expected. Packages not attempted after the first failure are recorded as blocked with an explanation, reported as skipped in progress totals, and remain eligible for a resumed publish.

**Before:**

```text
◆ Publish complete: 13 packages, ✅ 12 published, ⏭️ 0 skipped, ❌ 1 failed
```

**After:**

```text
◆ Publish complete: 45 expected, ✅ 12 succeeded, ❌ 1 failed, ⏭️ 32 skipped
```

The library also exposes a derived summary without changing the persisted report schema:

```rust
// before
let report: PackagePublishReport = execute_publish_requests(...).await?;

// after
let report: PackagePublishReport = execute_publish_requests(...).await?;
let summary: PackagePublishSummary = report.summary();
assert_eq!(summary.expected, 45);
assert_eq!(summary.succeeded, 12);
assert_eq!(summary.failed, 1);
assert_eq!(summary.skipped, 32);
```

Publish errors now include these aggregate counts and retain the failed package's command diagnostics. Command error rendering preserves stdout-only failures and clearly labels both streams when both are available.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #629](https://github.com/monochange/monochange/pull/629)

#### Remove colon-delimited built-in step compatibility

> **Breaking change** — colon-delimited top-level built-in step tokens are no longer accepted.
>
> Split each obsolete generated-step token into two arguments: the `step` namespace followed by the step name.

monochange now recognizes built-in steps only through the nested `step <name>` command tree. Obsolete colon-delimited names are no longer parsed, classified, reserved by configuration validation, or suggested by publishing errors, so scripts, telemetry, help text, and configuration all agree on one command shape.

Use the nested invocation:

```nu
monochange step validate
```

Update automation and argument arrays at the same boundary. For example, replace a single generated-step argument with two arguments: `["step", "validate"]`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #625](https://github.com/monochange/monochange/pull/625)

## [0.8.4](https://github.com/monochange/monochange/releases/tag/v0.8.4) (2026-07-11)

### 🐛 Fixed

#### report Dart trusted publishing authentication failures

Built-in package publishing now runs registry publish commands with stdin closed so commands cannot wait indefinitely for interactive authentication prompts in CI. This helps `dart pub publish --force` fail fast when pub.dev credentials or trusted-publishing context are missing instead of waiting until the workflow timeout.

When pub.dev publishing reports an authentication or credential error, monochange now appends guidance for the common trusted-publishing setup issues:

```text
pub.dev publishing could not authenticate non-interactively. If this package uses trusted publishing, verify the GitHub workflow has `id-token: write`, runs with the GitHub Actions environment configured on pub.dev, matches the package repository and tag/event policy, and runs `dart-lang/setup-dart` before `dart pub publish`.
```

Token fallback workflows are also pointed to the explicit pub token setup command:

```bash
dart pub token add https://pub.dev --env-var PUB_TOKEN
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #623](https://github.com/monochange/monochange/pull/623)

## [0.8.3](https://github.com/monochange/monochange/releases/tag/v0.8.3) (2026-06-29)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.8.3 as part of group `main`.

## [0.8.2](https://github.com/monochange/monochange/releases/tag/v0.8.2) (2026-06-18)

### 🐛 Fixed

#### Move rate-limit policy planning into publish core

Keep `monochange` as the CLI crate while moving publish rate-limit policy and window planning helpers into `monochange_publish`.

Ecosystem manifest update planning now lives in the relevant ecosystem crates with `monochange` acting as the CLI orchestrator. Hosted-source adapters now own release URL and release request planning behavior. The test helper crate also centralizes binary lookup for integration tests that need the `monochange` executable. `monochange_github` constrains the GitHub client transitive dependency set so release-job lockfile regeneration stays compatible with the pinned nightly toolchain.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #614](https://github.com/monochange/monochange/pull/614)

#### include CLI help markdown in published crate packages

The `monochange` crate package now includes markdown files from `src/` when Cargo builds the publish tarball. This keeps embedded CLI help text available to downstream builds and to `cargo publish` verification.

Before this fix, publishing could package `src/cli.rs` without the `src/cli_after_long_help.md` file referenced by `include_str!`, causing the crate verification step to fail with a missing-file error.

Command:

```bash
cargo publish --dry-run --manifest-path crates/monochange/Cargo.toml
```

After this change, the dry run can compile the packaged tarball because `src/cli_after_long_help.md` is included alongside the Rust sources.

The `monochange step publish-packages --stream-output` flag now streams package-manager stdout and stderr while commands run, while still capturing those streams in the publish report. The publish workflow enables this opt-in flag so Cargo verification errors from `cargo publish`, including missing packaged files, are visible in the normal CI log instead of only appearing in a later summarized report or annotation.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #615](https://github.com/monochange/monochange/pull/615)

## [0.8.1](https://github.com/monochange/monochange/releases/tag/v0.8.1) (2026-06-09)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.8.1 as part of group `main`.

## [0.8.0](https://github.com/monochange/monochange/releases/tag/v0.8.0) (2026-06-04)

### 🐛 Fixed

#### Update package documentation for the nested CLI command API

Updated generated package documentation, skill guidance, provider-facing examples, and release-record schema fixture text to refer to the new `monochange step <name>` and `monochange run <name>` command paths where those packages expose or document monochange CLI workflows.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #597](https://github.com/monochange/monochange/pull/597) · _Related issues:_ [#35](https://github.com/monochange/monochange/issues/35)

#### Add group package max bump controls

Allow version group package entries to use table syntax with `max_bump` so a member can cap how much its own changes raise the group version. String package entries keep the existing behavior and table entries default to `max_bump = "major"`; `max_bump = "none"` keeps the package aligned with the group without allowing that package's own changes to raise the group bump.

Rename CLI snapshot bump-cap fields from `max_semver_bump` to `max_bump`.

```json
{
	"commands": [
		{
			"path": ["experimental"],
			"max_bump": "minor"
		}
	]
}
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #602](https://github.com/monochange/monochange/pull/602)

## [0.7.0](https://github.com/monochange/monochange/releases/tag/v0.7.0) (2026-06-03)

### 🚀 Feature

#### Use snake_case for durable JSON schemas

Normalize durable monochange JSON schemas, release records, and CLI/report outputs to snake_case while preserving migration support for legacy camelCase release records.

```json
{
	"schema_version": "0.4",
	"kind": "monochange.release_record"
}
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #589](https://github.com/monochange/monochange/pull/589)

### 🐛 Fixed

#### Add npm placeholder publish OTP support

Allow npm placeholder publishing to receive a one-time password for accounts that require 2FA during publish operations.

Before, `mc placeholder-publish` and `mc step:placeholder-publish` could only invoke `npm publish` without an OTP, causing npm `EOTP` failures for publish-time 2FA accounts.

After, pass a fresh code with `--otp`:

```sh
mc placeholder-publish --otp 123456
mc step:placeholder-publish --from HEAD --otp 123456
```

The generated npm process receives the code through `NPM_CONFIG_OTP`, keeping it out of command arguments, reports, and failure messages.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #590](https://github.com/monochange/monochange/pull/590)

## [0.6.8](https://github.com/monochange/monochange/releases/tag/v0.6.8) (2026-05-31)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.6.8 as part of group `main`.

## [0.6.7](https://github.com/monochange/monochange/releases/tag/v0.6.7) (2026-05-30)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.6.7 as part of group `main`.

## [0.6.6](https://github.com/monochange/monochange/releases/tag/v0.6.6) (2026-05-29)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.6.6 as part of group `main`.

## [0.6.5](https://github.com/monochange/monochange/releases/tag/v0.6.5) (2026-05-29)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.6.5 as part of group `main`.

## [0.6.4](https://github.com/monochange/monochange/releases/tag/v0.6.4) (2026-05-28)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.6.4 as part of group `main`.

## [0.6.3](https://github.com/monochange/monochange/releases/tag/v0.6.3) (2026-05-28)

### Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.6.3 as part of group `main`.

## [0.6.2](https://github.com/monochange/monochange/releases/tag/v0.6.2) (2026-05-27)

### 🐛 Fixed

#### Refresh documentation audit coverage

Updates documentation, CLI help text, package README content, and packaged skill guidance so the documented command surface matches the current monochange CLI and release workflow behavior.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #546](https://github.com/monochange/monochange/pull/546)

#### Include required files in placeholder publish directories

Placeholder publish directories now include a `LICENSE` and `CHANGELOG.md` alongside the placeholder `README.md` and registry manifest. This lets Dart placeholder packages pass pub.dev's required-file validation during `mc step:placeholder-publish`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #547](https://github.com/monochange/monochange/pull/547)

#### Fix placeholder publish skipping external-mode packages

Previously, `mc step:placeholder-publish` skipped packages configured with `publish.mode = "external"`, showing messages like "package opted out of built-in publishing". This was incorrect because placeholder publishing is a bootstrap utility separate from normal release publishing.

Now placeholder publishing proceeds for all publishable packages regardless of `publish.mode`. The following safeguards remain in effect:

- `publish.enabled = false` still opts out completely
- Private/unpublishable package metadata is still respected
- Registry support limitations are still enforced

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #543](https://github.com/monochange/monochange/pull/543) · _Closed issues:_ [#542](https://github.com/monochange/monochange/issues/542)

## [0.6.1](https://github.com/monochange/monochange/releases/tag/v0.6.1) (2026-05-24)

### 🚀 Feature

#### Resilient discovery and Dart/Flutter ecosystem unification

###### Discovery no longer crashes on unfamiliar ecosystems

`mc init` and `discover_all` now gracefully handle errors from individual ecosystem adapters. When an adapter (e.g. npm) fails in a monorepo that doesn't use that ecosystem (e.g. a Dart monorepo), the error is logged as a warning and discovery continues with remaining adapters instead of aborting. The npm adapter's `expand_member_patterns` also guards against workspace glob patterns that resolve to directories without a `package.json`.

###### Flutter merged into the Dart ecosystem

`Ecosystem::Flutter` and `PackageType::Flutter` have been removed. Flutter packages use `Ecosystem::Dart` with an `is_flutter` metadata flag on the package record, since Flutter and Dart share `pubspec.yaml`, `pub.dev`, and the same tooling. The publish and lockfile commands now check this metadata to choose `flutter pub get`/`flutter pub publish` vs `dart pub get`/`dart pub publish`. Config files that use the string `"flutter"` are deserialized to `Ecosystem::Dart` or `PackageType::Dart` for backward compatibility.

```toml
# Before (still works, maps to dart ecosystem):
[[packages]]
type = "flutter"

# After (preferred):
[[packages]]
type = "dart"
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #530](https://github.com/monochange/monochange/pull/530) _Introduced in:_ [`7a1ef20`](https://github.com/monochange/monochange/commit/7a1ef2061ac22a0bb9918b113009d468aa471083)

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

#### Add prerelease mode

Add first-class prerelease configuration and release planning support.

Prerelease mode now writes `.monochange/prerelease-state.json`, preserves the original stable baseline across repeated prerelease preparations, supports planned/current/fixed stable bases, and can synthesize prerelease plans without changesets.

Validation now rejects stale prerelease state when prerelease mode is disabled, stable release preparation removes the prerelease state file, and `[prerelease].branches` can override stable release branch restrictions for prerelease tag/publish steps.

Enable incrementing alpha prereleases from the next planned stable version:

```toml
[prerelease]
enabled = true
channel = "alpha"
numbering = "increment"
base = "planned"
branches = ["next", "prerelease/*"]
```

Use release-candidate prereleases from the current stable baseline when you want a tagged binary build without applying changeset bump severity yet:

```toml
[prerelease]
enabled = true
channel = "rc"
numbering = "increment"
base = "current-stable"
publish_packages = false
```

Use a fixed `0.0.0` nightly-style prerelease line with date-based identifiers:

```toml
[prerelease]
enabled = true
channel = "nightly"
numbering = "date"
base = "fixed"
base_version = "0.0.0"
keep_changesets = true
changelog = false
release_notes = true
publish_packages = false
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #522](https://github.com/monochange/monochange/pull/522) _Introduced in:_ [`9a5fe30`](https://github.com/monochange/monochange/commit/9a5fe305600c17364f8916fe9cfc160825dfda5c) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 📝 Changed

- No package-specific changes were recorded; `monochange_publish` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 🚀 Feature

#### Configurable publish-order dependency fields

Add configurable ecosystem-specific dependency fields for package publish ordering across npm, Cargo, Deno, Dart/Flutter, Python, and Go.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #472](https://github.com/monochange/monochange/pull/472) _Introduced in:_ [`0d9cf46`](https://github.com/monochange/monochange/commit/0d9cf461a05057b61efa987d361ebd27d800dbdb) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#465](https://github.com/monochange/monochange/issues/465)

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Add progress logging to `mc publish`

When running `mc publish`, each package being processed is now logged via `tracing::info!` so users can observe progress in real time. Use `--log-level info` or set `RUST_LOG=info` to see these messages. When `--quiet` is set, no tracing subscriber is initialized so the log messages are silently discarded (zero overhead).

Log events emitted during the publish loop:

- **`publishing package`** — at the start of processing each package, with `package_name`, `version`, `registry`, `dry_run`, and `mode` fields
- **`skipping external package`** — when a package opts out of built-in publishing
- **`skipping already-published version`** — when the version already exists on the registry
- **`would publish package (dry run)`** — when `--dry-run` would publish the package
- **`published package`** — on successful publish
- **`publish command failed to execute`** (`tracing::error`) — when the publish command cannot run
- **`publish command returned non-zero exit`** (`tracing::error`) — when the publish command fails

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #453](https://github.com/monochange/monochange/pull/453) _Introduced in:_ [`586ffb6`](https://github.com/monochange/monochange/commit/586ffb6b61c7f61b0a6bbcafc8dc2dbfa66d7203) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Remove automated npm trust configuration during publish

Removed the `npm trust` command execution from the publish loop. Trust configuration for npm packages must now be done manually or via separate tooling — `mc publish` no longer runs `npm trust github` or `npm trust list` automatically.

When trusted publishing is enabled for npm packages, the publish command now uses `npm` directly instead of `pnpm` (already the case via `npm_publish_program`). An environment variable override for forcing pnpm during trusted publishing can be added in a future release.

Removed `PublishTrustHandler::configure_successful_publish_trust` from the trait and its `CliPublishTrustHandler` implementation. Removed `configure_npm_trusted_publishing` from `package_publish`. Removed `build_npm_trust_list_command` from `monochange_npm`. The `trust_outcome_for_skip` and `planned_trust_outcome` methods remain, showing informational messages about how to manually configure trust.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #456](https://github.com/monochange/monochange/pull/456) _Introduced in:_ [`628a1ea`](https://github.com/monochange/monochange/commit/628a1ea18b62b60551c7648e16405a685cacb5f4) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🐛 Fixed

#### Include Cargo development dependencies in publish ordering

Cargo package publishing now orders runtime, build, and development dependencies before dependents. This prevents a crate from being published before an unpublished workspace crate referenced through `dev-dependencies` or `build-dependencies`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #466](https://github.com/monochange/monochange/pull/466) _Introduced in:_ [`add0671`](https://github.com/monochange/monochange/commit/add0671b798d2dd4ab6e142801b1b5cac6842a1a) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Validate Cargo private dependency publishing hazards

Cargo linting now reports publishable packages that depend on private workspace packages through `dependencies`, `dev-dependencies`, or `build-dependencies`. Package publish dry runs now execute the registry dry-run command and preserve its stdout and stderr in the publish report instead of only planning the command.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #470](https://github.com/monochange/monochange/pull/470) _Introduced in:_ [`66ffdf7`](https://github.com/monochange/monochange/commit/66ffdf734129fb267fe61dd821e55c292dab5c0e) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Publish progress output

Add emoji-based publish progress reporting on stderr with deterministic CI-friendly output and terminal-aware loading markers.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #469](https://github.com/monochange/monochange/pull/469) _Introduced in:_ [`603c731`](https://github.com/monochange/monochange/commit/603c731a60d66f49b876a14467909efd4585408a) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### 🚀 Feature

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### 🐛 Fixed

#### Split crate boundaries for changelog, config, and publish behavior

Move changelog rendering into `monochange_changelog`, shift publish planning and execution helpers into `monochange_publish`, and reduce direct concrete ecosystem/provider dependencies in `monochange_config`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #441](https://github.com/monochange/monochange/pull/441) _Introduced in:_ [`ae8ea56`](https://github.com/monochange/monochange/commit/ae8ea563ae95c6cc4e8d3d1acdc5303069ea44cf)

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### 💥 Breaking Change

#### Extract publish support into a dedicated crate

Move the publish support surface out of the top-level `monochange` crate and into the new `monochange_publish` crate. The extracted crate now owns the publish report/request models, trusted-publishing capability detection, provider/registry capability messages, and built-in publish command builders for npm, pnpm, Cargo, Dart, Flutter, JSR, PyPI, and Go proxy releases.

This keeps `monochange` focused on orchestration while giving publish integrations a dedicated crate boundary for future registry checks, readiness logic, and provider-specific publishing workflows.

```text
monochange_publish owns reusable publish capabilities and command construction.
monochange wires those capabilities into CLI workflows and release orchestration.
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #397](https://github.com/monochange/monochange/pull/397) _Introduced in:_ [`fa78e4d`](https://github.com/monochange/monochange/commit/fa78e4db56fd3a6897896c6e1b1c62ea2d8e46b9) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### 🚀 Feature

#### Consolidate adapter traits to remove ecosystem match arms

Replace hardcoded ecosystem and registry match arms in `workspace_ops`, `monochange_config`, and `monochange_publish` with adapter registry dispatch.

- Expand `EcosystemAdapter` in `monochange_core` with `load_configured`, `supported_versioned_file_kind`, and `validate_versioned_file`.
- Add `From<EcosystemType>` and `From<PackageType>` conversions for `Ecosystem`.
- Add `FromStr` for `Ecosystem` and extract `default_registry_kind_for_ecosystem` into `monochange_core`.
- Implement the new trait methods in all ecosystem adapter crates.
- Replace `discover_packages` body with `build_ecosystem_registry().discover_all(root)?`.
- Replace `discover_release_workspace` `load_configured` match arms with registry dispatch.
- Replace `path_is_supported_for_ecosystem` and `validate_ecosystem_version_readable` match arms in `monochange_config` with registry dispatch.
- Introduce `PublishAdapter` trait and `PublishCommandBuilder` in `monochange_publish` to replace `build_publish_command` registry match arms.
- Extract `default_registry_kind_for_ecosystem` mapping out of `package_publish.rs` into `monochange_core`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #429](https://github.com/monochange/monochange/pull/429) _Introduced in:_ [`271e554`](https://github.com/monochange/monochange/commit/271e55420154265e798a0de3adf26a64faba66c8)

#### Move CommandExecutor and command rendering into monochange_publish

Extract `CommandOutput`, `CommandExecutor`, `ProcessCommandExecutor`, and the helper functions `render_command` and `render_command_error` from `monochange::package_publish` into `monochange_publish`. This continues the Phase 2 crate boundary cleanup by ensuring the publish crate owns all command execution infrastructure used during publishing.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #409](https://github.com/monochange/monochange/pull/409) _Introduced in:_ [`f08c48b`](https://github.com/monochange/monochange/commit/f08c48be727539436ba7d839fa93a6ca5df7d0bb)

#### Move registry infrastructure from `monochange` into `monochange_publish`

This change relocates registry-facing utilities so the publish crate owns all HTTP transport and registry endpoint concerns:

- `RegistryEndpoints` – configurable registry base URLs with environment fallbacks
- `registry_client()` – shared blocking HTTP client with monochange user-agent
- `package_can_be_published()` – predicate that checks publish enablement and state
- `filter_pending_publish_requests()` – filters out already-published or external entries
- `filter_pending_publish_requests_with_transport()` – same with transport-aware checks
- `registry_version_exists()` – ecosystem-aware version existence probe
- `crates_io_version_exists()` – Crates.io API version lookup with index fallback
- `crates_io_index_version_exists()` – sparse-index version existence check
- `crates_io_index_entry_path()` – sparse-index path computation for a crate name

`monochange` now delegates to these via `monochange_publish` imports rather than owning the implementation. `publish_rate_limits.rs` also imports them from `monochange_publish` instead of `package_publish` directly.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #404](https://github.com/monochange/monochange/pull/404) _Introduced in:_ [`7b09570`](https://github.com/monochange/monochange/commit/7b09570cd076b97c49210b6f3e1aeb33fb7eaf68)

#### Move resume and dependency ordering to monochange_publish

Move resume/artifact logic (`read_publish_report_artifact`, `write_publish_report_artifact`, `ensure_publish_report_succeeded`, `resume_publish_requests`, `merge_publish_resume_report`) and dependency ordering (`order_release_requests_by_publish_dependencies`, `render_publish_dependency_cycle`) from `monochange` into `monochange_publish`.

This continues the Phase 2 crate boundary audit by removing more publish-orchestration helpers from the top-level `monochange` crate into the dedicated `monochange_publish` crate where they belong.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #412](https://github.com/monochange/monochange/pull/412) _Introduced in:_ [`86cbd66`](https://github.com/monochange/monochange/commit/86cbd668fbbd1ce20154a7b3102eed18e26209a8)

### 🐛 Fixed

#### Move Cargo publish readiness blockers into monochange_cargo

Move `cargo_publish_readiness_blockers` and workspace package table helpers (`read_workspace_package_table`, `maybe_read_workspace_manifest_contents`, `parse_workspace_manifest_value`, `extract_workspace_package_table`) from the top-level `monochange` crate into `monochange_cargo`.

Also fixes a clippy `indexing_slicing` lint in `monochange_publish` that was introduced by the previous resume/dependency-ordering extraction.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #413](https://github.com/monochange/monochange/pull/413) _Introduced in:_ [`904ba37`](https://github.com/monochange/monochange/commit/904ba37962c1fb2db7af87ebfa2ef80230c780a5)

#### Remove grouped release member summaries

Grouped release notes no longer include generated changed or synchronized member lists, keeping the release note summary focused on the group release itself.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #395](https://github.com/monochange/monochange/pull/395) _Introduced in:_ [`2d012ff`](https://github.com/monochange/monochange/commit/2d012ff900a612f4aed6e4d7034c8c876f50aeae) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### 🧪 Testing

#### Extract inline test modules into separate files

Move all inline `#[cfg(test)] mod tests { ... }` blocks out of source files into dedicated test files. This reduces source file sizes and keeps test code in a consistent `__tests/` directory structure next to the module it tests.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #416](https://github.com/monochange/monochange/pull/416) _Introduced in:_ [`3535c88`](https://github.com/monochange/monochange/commit/3535c887c46d66db2768377cb5f01406f6e9a8b6)

#### Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #428](https://github.com/monochange/monochange/pull/428) _Introduced in:_ [`b61cc3e`](https://github.com/monochange/monochange/commit/b61cc3e66989fd83ffb16a31568d2f46d7075216)
