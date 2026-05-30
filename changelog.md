# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.6.7](https://github.com/monochange/monochange/releases/tag/v0.6.7) (2026-05-30)

Grouped release for `main`.

### 🚀 Feature

#### Add generic format versioned files

_Packages:_ _monochange_, _monochange_config_, _monochange_core_

Add `versioned_files` format mode for explicit version updates in JSON, TOML, YAML/YML, and env files without ecosystem-specific dependency handling.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #569](https://github.com/monochange/monochange/pull/569)

### 🐛 Fixed

#### Include mc in release archives for cargo-binstall

_Packages:_ _monochange_

Ship both `monochange` and `mc` binaries in GitHub release archives so `cargo binstall monochange` can install the non-optional `mc` binary.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #566](https://github.com/monochange/monochange/pull/566)

#### Speed up CLI startup

_Packages:_ _monochange_, _monochange_config_

Version and help paths now avoid full workspace validation. Full config loading now deduplicates inherited versioned-file glob validation and resolves glob checks against one ignored-file-aware workspace walk instead of repeatedly scanning the filesystem.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #568](https://github.com/monochange/monochange/pull/568)

#### Speed up release command step evaluation

_Packages:_ _monochange_

Avoid building full template contexts for simple release command steps, literal commands, and direct input forwarding. This makes dry-run releases in large workspaces surface progress immediately and evaluate skipped command steps much faster.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #565](https://github.com/monochange/monochange/pull/565)

<details>
<summary><strong>📖 Documentation</strong></summary>

#### Document lint rule catalog

_Packages:_ _monochange_

Expand the linting reference with the available presets, every built-in lint rule, and the `changesets/summary.require_description` option.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #570](https://github.com/monochange/monochange/pull/570)

</details>

## [0.6.6](https://github.com/monochange/monochange/releases/tag/v0.6.6) (2026-05-29)

Grouped release for `main`.

### 🐛 Fixed

#### Preserve native manifest updates before versioned_files

_Packages:_ _monochange_

Apply `versioned_files` changes on top of native manifest updates so dependency constraints can be rewritten without clobbering package version fields.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #562](https://github.com/monochange/monochange/pull/562)

#### Fix versioned_files clobbering version field

_Packages:_ _monochange_

Prevent `versioned_files` from updating the `version` field in native manifests unless explicitly specified in the `fields` configuration. This applies to all ecosystems: Cargo, Dart, Deno, npm, Python, and Go.

Previously, the version field would be overwritten whenever a group had `versioned_files` listed, even without `version` in the `fields` array.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #560](https://github.com/monochange/monochange/pull/560)

#### Skip config loading for --version and --help flags

_Packages:_ _monochange_

Previously, every CLI invocation loaded workspace configuration from disk before parsing arguments. This meant `mc --version` and `mc --help` paid the cost of reading and parsing monochange.toml even though they don't need configuration.

The fix adds a fast path that parses arguments with the base command (no config-loaded subcommands) first. If the result is --version or root-level --help, it returns immediately without touching disk.

Benchmark results (release build, 50 runs each):

- --version: 8ms (was already fast in release, but avoids config I/O)
- --help: 8ms
- init --help: 9ms
- check --help: 9ms
- step:validate --help: 9ms

Also exports build_command_for_root for production use and adds scripts/benchmark-commands.sh for PR regression detection.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #561](https://github.com/monochange/monochange/pull/561)

#### Skip async initialization for --version flag

_Packages:_ _monochange_

Previously, `mc --version` initialized the full Tokio runtime, rustls crypto provider, and tracing subscriber before printing the version. Now a synchronous fast path checks for --version/-V before any async initialization, reducing latency from ~7ms to ~4ms.

Also removed the redundant rustls crypto provider installation from run_cli_binary_from_env — it's already lazily installed by build_http_client in monochange_hosting when an HTTP request is made.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #563](https://github.com/monochange/monochange/pull/563)

## [0.6.5](https://github.com/monochange/monochange/releases/tag/v0.6.5) (2026-05-29)

Grouped release for `main`.

### 🚀 Feature

#### Add require_description to summary lint rule

_Packages:_ _monochange_config_

The `changesets/summary` lint rule now supports a `require_description` option that ensures the summary heading is followed by at least one non-empty paragraph (not another heading). When enabled, a changeset with only a heading and no description body will fail validation.

Additionally, `max_length` now defaults to 60 characters when the rule is activated. Users can override this by setting `max_length` explicitly in the rule options.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #555](https://github.com/monochange/monochange/pull/555)

### 🐛 Fixed

#### Add JSON schema annotation to init template

_Packages:_ _monochange_

The generated `monochange.toml` now includes a `#:schema` directive at the top, enabling automatic validation in editors that support JSON Schema annotations (e.g., VS Code with Even Better TOML).

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #556](https://github.com/monochange/monochange/pull/556)

#### Fix versioned_files clobbering version field

_Packages:_ _monochange_

Prevent `versioned_files` from updating the `version` field in native manifests unless explicitly specified in the `fields` configuration. This applies to all ecosystems: Cargo, Dart, Deno, npm, Python, and Go.

Previously, the version field would be overwritten whenever a group had `versioned_files` listed, even without `version` in the `fields` array.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #560](https://github.com/monochange/monochange/pull/560)

#### Eliminate redundant directory traversals in ecosystem discovery

_Packages:_ _monochange_, _monochange_cargo_, _monochange_dart_, _monochange_deno_, _monochange_npm_

Each ecosystem adapter (Dart, Cargo, Deno, npm) previously called `find_all_manifests` twice during package discovery — once to find workspace manifests and again to find all manifests for standalone packages. This doubled the wall-clock time for large monorepos.

The fix refactors each adapter to call `find_all_manifests` once and reuse the results for both workspace and standalone discovery, and removes the now-unused `find_workspace_manifests` helper functions.

Benchmark results (51-package Dart monorepo):

- Before: ~40ms
- After: ~14ms (65% improvement)

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #558](https://github.com/monochange/monochange/pull/558)

## [0.6.4](https://github.com/monochange/monochange/releases/tag/v0.6.4) (2026-05-28)

Grouped release for `main`.

### 🐛 Fixed

#### Fix ANSI color bleeding between CLI step outputs

_Packages:_ _monochange_

When subprocess output (e.g. from cargo, git) contained ANSI color codes without trailing reset sequences, the color state would leak into subsequent step output, spinner rendering, and progress lines. This caused brownish/yellowish color bleeding visible when Prepare Release ran before other steps.

Added `\x1b[0m` (ANSI reset) to all output paths:

- Raw subprocess log lines in `log_command_output`
- Line-clear sequences in `print_line` and `stop_spinner`
- Spinner rendering in the animation thread
- Phase timing detail lines

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #553](https://github.com/monochange/monochange/pull/553)

## [0.6.3](https://github.com/monochange/monochange/releases/tag/v0.6.3) (2026-05-28)

Grouped release for `main`.

### 🚀 Feature

#### Polish `mc versions` command

_Packages:_ _monochange_, _monochange_core_

Rename `mc sync versions` to top-level `mc versions` with plan/apply abstraction, `--format text|json` output, unsupported ecosystem reporting, and snapshot-tested CLI output. Add Criterion benchmark coverage and documentation.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #540](https://github.com/monochange/monochange/pull/540) · _Related issues:_ [#539](https://github.com/monochange/monochange/issues/539)

### 🐛 Fixed

#### Document `mc versions` command with full ecosystem coverage

_Packages:_ _@monochange/skill_, _monochange_

Update documentation and skill to accurately describe the `mc versions` command:

- **All ecosystems supported**: Cargo, Dart, Deno, Go, npm, and Python
- **Usage examples**: `--dry-run`, `--format json`, `--strategy exact/caret/compatible`
- **Ecosystem details**: Each adapter's manifest scanning behavior documented

Fix incorrect reference in start-here guide that described `mc versions` as read-only (it actually writes to manifests).

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #552](https://github.com/monochange/monochange/pull/552)

#### Polish `mc versions` command

_Packages:_ _@monochange/skill_

Rename `mc sync versions` to top-level `mc versions` with plan/apply abstraction, `--format text|json` output, unsupported ecosystem reporting, and snapshot-tested CLI output. Add Criterion benchmark coverage and documentation.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #540](https://github.com/monochange/monochange/pull/540) · _Related issues:_ [#539](https://github.com/monochange/monochange/issues/539)

#### Support `--version` flag on mc CLI

_Packages:_ _monochange_

The root clap command was missing `.version()`, causing `mc --version` to be rejected as an unexpected argument. Added `CARGO_PKG_VERSION` registration so the flag now works correctly.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #551](https://github.com/monochange/monochange/pull/551)

#### Filter group-propagated changes from per-package changelogs

_Packages:_ _monochange_changelog_

When a package is a member of a version group, its per-package changelog now only includes changes from changesets that directly target that package (kind=Package), not changes propagated from group-level targeting (kind=Group). Group-level changes appear exclusively in the group changelog, eliminating content duplication across member changelogs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #549](https://github.com/monochange/monochange/pull/549) · _Closed issues:_ [#548](https://github.com/monochange/monochange/issues/548)

## [0.6.2](https://github.com/monochange/monochange/releases/tag/v0.6.2) (2026-05-27)

Grouped release for `main`.

### 🚀 Feature

#### Add `mc sync versions` command for internal dependency synchronization

_Packages:_ _monochange_, _monochange_core_, _monochange_dart_, _monochange_npm_

Add a new CLI subcommand `mc sync versions` that synchronizes internal dependency version references across workspace packages to match each package's canonical version.

- **`VersionStrategy` enum** in `monochange_core` controls constraint format: `Default`, `Exact`, `Caret`, `Compatible`.
- **`DependencySyncChange` struct** in `monochange_core` reports what changed (dependency name, section, old value, new value).
- **`sync_internal_dependency_versions()`** in `monochange_dart` scans pubspec.yaml `dependencies`, `dev_dependencies`, and `dependency_overrides` for internal workspace references and computes the target version constraint. Under `resolution: workspace`, `path:` references are converted to versioned constraints.
- **`sync_internal_dependency_versions()`** in `monochange_npm` scans package.json for internal workspace dependencies, skipping `workspace:*` protocol references.
- **CLI subcommand** `mc sync versions [--dry-run] [--strategy <default|exact|caret|compatible>]` orchestrates discovery, version map building, and per-ecosystem sync.
- **`--dry-run`** flag shows what would change without writing files.

Currently supports **Dart** and **npm** ecosystems. Other ecosystems will be added in follow-up changes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #538](https://github.com/monochange/monochange/pull/538) · _Related issues:_ [#536](https://github.com/monochange/monochange/issues/536), [#537](https://github.com/monochange/monochange/issues/537)

#### Add `Inline` metadata style and make it the default

_Packages:_ _monochange_changelog_, _monochange_core_

Context blocks in changelog entries now render as a single inline paragraph joined with `·` instead of separate lines.

When a review request (PR/MR) link is available, commit links are omitted since the PR already identifies the change. When no review request link exists, commit links are included as before.

The existing `Plain` and `Blockquote` styles continue to render commit links unconditionally. The `Omit` style hides all metadata as before.

**Before (default: `plain`):**

```markdown
# Add release summary panel

_Owner:_ @user _Review:_ [PR #123](https://...) _Introduced in:_ [`abc1234`](https://...) _Related issues: #456
```

**After (default: `inline`):**

```markdown
# Add release summary panel

_Owner:_ @user · _Review:_ [PR #123](https://...) · _Related issues: #456
```

Set `metadata_style = "inline"` (now the default), `"plain"`, `"blockquote"`, or `"omit"` under `[changelog.style]` in `monochange.toml`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #532](https://github.com/monochange/monochange/pull/532) · _Related issues:_ [#123](https://github.com/monochange/monochange/issues/123), [#456](https://github.com/monochange/monochange/issues/456)

### 🐛 Fixed

#### Refresh documentation audit coverage

_Packages:_ _@monochange/cli_, _@monochange/skill_, _monochange_, _monochange_cargo_, _monochange_config_, _monochange_core_, _monochange_dart_, _monochange_deno_, _monochange_forgejo_, _monochange_gitea_, _monochange_github_, _monochange_gitlab_, _monochange_graph_, _monochange_hosting_, _monochange_npm_, _monochange_publish_, _monochange_semver_, _monochange_telemetry_, _monochange_test_helpers_

Updates documentation, CLI help text, package README content, and packaged skill guidance so the documented command surface matches the current monochange CLI and release workflow behavior.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #546](https://github.com/monochange/monochange/pull/546)

#### Add `mc sync versions` command for internal dependency synchronization

_Packages:_ _@monochange/skill_

Add a new CLI subcommand `mc sync versions` that synchronizes internal dependency version references across workspace packages to match each package's canonical version.

- **`VersionStrategy` enum** in `monochange_core` controls constraint format: `Default`, `Exact`, `Caret`, `Compatible`.
- **`DependencySyncChange` struct** in `monochange_core` reports what changed (dependency name, section, old value, new value).
- **`sync_internal_dependency_versions()`** in `monochange_dart` scans pubspec.yaml `dependencies`, `dev_dependencies`, and `dependency_overrides` for internal workspace references and computes the target version constraint. Under `resolution: workspace`, `path:` references are converted to versioned constraints.
- **`sync_internal_dependency_versions()`** in `monochange_npm` scans package.json for internal workspace dependencies, skipping `workspace:*` protocol references.
- **CLI subcommand** `mc sync versions [--dry-run] [--strategy <default|exact|caret|compatible>]` orchestrates discovery, version map building, and per-ecosystem sync.
- **`--dry-run`** flag shows what would change without writing files.

Currently supports **Dart** and **npm** ecosystems. Other ecosystems will be added in follow-up changes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #538](https://github.com/monochange/monochange/pull/538) · _Related issues:_ [#536](https://github.com/monochange/monochange/issues/536), [#537](https://github.com/monochange/monochange/issues/537)

#### Add `Inline` metadata style and make it the default

_Packages:_ _monochange_

Context blocks in changelog entries now render as a single inline paragraph joined with `·` instead of separate lines.

When a review request (PR/MR) link is available, commit links are omitted since the PR already identifies the change. When no review request link exists, commit links are included as before.

The existing `Plain` and `Blockquote` styles continue to render commit links unconditionally. The `Omit` style hides all metadata as before.

**Before (default: `plain`):**

```markdown
# Add release summary panel

_Owner:_ @user _Review:_ [PR #123](https://...) _Introduced in:_ [`abc1234`](https://...) _Related issues: #456
```

**After (default: `inline`):**

```markdown
# Add release summary panel

_Owner:_ @user · _Review:_ [PR #123](https://...) · _Related issues: #456
```

Set `metadata_style = "inline"` (now the default), `"plain"`, `"blockquote"`, or `"omit"` under `[changelog.style]` in `monochange.toml`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #532](https://github.com/monochange/monochange/pull/532) · _Related issues:_ [#123](https://github.com/monochange/monochange/issues/123), [#456](https://github.com/monochange/monochange/issues/456)

#### Enforce version constraints for Dart workspace resolution internal deps

_Packages:_ _monochange_, _monochange_dart_

The `dart/internal-path-dependency-policy` lint rule now enforces version constraints (not `path:` references) when a pubspec declares `resolution:
workspace`. Dart workspace resolution resolves versioned internal dependencies to local workspace packages automatically, so `path:` references are redundant and can cause publishing issues.

**Before:** With `resolution: workspace`, internal deps using either `path:` or version constraints would pass the lint.

**After:** With `resolution: workspace`, internal deps must use version constraints — `path:` references now produce a lint failure with the message "use version constraints (not `path:`) when resolution is workspace".

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #536](https://github.com/monochange/monochange/pull/536)

#### Allow Dart workspace resolution for internal dependencies

_Packages:_ _monochange_, _monochange_dart_

Dart linting now treats `resolution: workspace` as a valid internal package resolution mode, so versioned internal dependencies in `pubspec.yaml` files no longer fail the internal path dependency policy when Dart will resolve them from the workspace.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #534](https://github.com/monochange/monochange/pull/534)

#### Skip gitignored release staging paths

_Packages:_ _monochange_, _monochange_github_

Release staging now skips gitignored paths before git inspection, avoiding failures on ignored symlink descendants such as FVM Flutter SDK files.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #545](https://github.com/monochange/monochange/pull/545) · _Closed issues:_ [#541](https://github.com/monochange/monochange/issues/541)

#### Fix placeholder publish skipping external-mode packages

_Packages:_ _monochange_, _monochange_core_, _monochange_publish_

Previously, `mc step:placeholder-publish` skipped packages configured with `publish.mode = "external"`, showing messages like "package opted out of built-in publishing". This was incorrect because placeholder publishing is a bootstrap utility separate from normal release publishing.

Now placeholder publishing proceeds for all publishable packages regardless of `publish.mode`. The following safeguards remain in effect:

- `publish.enabled = false` still opts out completely
- Private/unpublishable package metadata is still respected
- Registry support limitations are still enforced

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #543](https://github.com/monochange/monochange/pull/543) · _Closed issues:_ [#542](https://github.com/monochange/monochange/issues/542)

#### Add environment constraints to Dart placeholder manifests

_Packages:_ _monochange_dart_

Generated Dart and Flutter placeholder `pubspec.yaml` files now reuse the source package's `environment` block when available, falling back to safe Dart/Flutter SDK constraints when it is missing.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #544](https://github.com/monochange/monochange/pull/544)

#### Include required files in placeholder publish directories

_Packages:_ _monochange_publish_

Placeholder publish directories now include a `LICENSE` and `CHANGELOG.md` alongside the placeholder `README.md` and registry manifest. This lets Dart placeholder packages pass pub.dev's required-file validation during `mc step:placeholder-publish`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #547](https://github.com/monochange/monochange/pull/547)

## [0.6.1](https://github.com/monochange/monochange/releases/tag/v0.6.1) (2026-05-24)

Grouped release for `main`.

### 🚀 Feature

#### Resilient discovery and Dart/Flutter ecosystem unification

_Packages:_ _monochange_, _monochange_config_, _monochange_core_, _monochange_dart_, _monochange_npm_, _monochange_publish_

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

### 🐛 Fixed

#### Refactor npm scripts to TypeScript

_Packages:_ _@monochange/cli_

Move repository npm tooling and the npm CLI launcher source to TypeScript so local and CI scripts run through Node's native TypeScript support while the published CLI package still ships a built JavaScript bin.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #529](https://github.com/monochange/monochange/pull/529) _Introduced in:_ [`99cdf08`](https://github.com/monochange/monochange/commit/99cdf08d560d40c020d9bf031c0441fd871d67e4)

## [0.6.0](https://github.com/monochange/monochange/releases/tag/v0.6.0) (2026-05-23)

Grouped release for `main`.

### 💥 Breaking Change

#### Async migration: Tokio async runtime end-to-end

_Packages:_ _monochange_, _monochange_analysis_, _monochange_config_, _monochange_core_, _monochange_forgejo_, _monochange_gitea_, _monochange_github_, _monochange_gitlab_, _monochange_hosting_, _monochange_publish_

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

### 🚀 Feature

#### Add prerelease mode

_Packages:_ _monochange_, _monochange_config_, _monochange_core_

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

#### Add configurable changelog rendering styles

_Packages:_ _monochange_, _monochange_changelog_, _monochange_config_, _monochange_core_

Add configurable changelog and release-note rendering style options for section separators, package labels, metadata lines, and collapsed sections.

```toml
[changelog.style]
sectionSeparator = "blank_line"
packageLabelStyle = "inline"
packageLabelPlacement = "after_heading"
metadataStyle = "plain"
collapsedSectionStyle = "details"

[changelog.release_notes]
metadataStyle = "blockquote"
```

The config schema now includes `ChangelogStyle` and `ReleaseNotesStyleOverrides`, with release notes inheriting `[changelog.style]` unless a field-specific override is set.

Default section headings now include emoji in the `heading` string, while the stable section keys remain unchanged:

- `breaking`: `💥 Breaking Change`
- `feat`: `🚀 Feature`
- `change`: `📝 Changed`
- `fix`: `🐛 Fixed`
- `test`: `🧪 Testing`
- `refactor`: `🔨 Refactor`
- `docs`: `📖 Documentation`
- `security`: `🔒 Security`
- `perf`: `⚡ Performance`
- `none`: `🔖 None`

Semver level type aliases route to semantic sections: `major` to `breaking`, `minor` to `feat`, and `patch` to `fix`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #511](https://github.com/monochange/monochange/pull/511) _Introduced in:_ [`b03612b`](https://github.com/monochange/monochange/commit/b03612b5d69f05becd68a803efa535e0f874ee01) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

#### add semantic semver guardrails to release planning

_Packages:_ _monochange_, _monochange_semver_

Release planning now folds semantic analyzer evidence into compatibility assessments so public API and export diffs can raise the planned bump during previews. The guardrail is advisory: analyzer failures and uncovered semantic changes are reported as warnings instead of blocking release planning.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #523](https://github.com/monochange/monochange/pull/523) _Introduced in:_ [`e7fcb04`](https://github.com/monochange/monochange/commit/e7fcb04f9b80b9e1578a8bb4801fde80e59aec18) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63) _Closed issues:_ [#249](https://github.com/monochange/monochange/issues/249)

### 🐛 Fixed

#### Add `nix` / `devenv` installation instructions

_Packages:_ _@monochange/skill_

Mention the `ifiokjr/nixpkgs` flake overlay.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #519](https://github.com/monochange/monochange/pull/519) _Introduced in:_ [`23bd962`](https://github.com/monochange/monochange/commit/23bd962434001e4293abdd8e9d33cf185cab3221) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

#### add semantic semver guardrails to release planning

_Packages:_ _@monochange/skill_

Release planning now folds semantic analyzer evidence into compatibility assessments so public API and export diffs can raise the planned bump during previews. The guardrail is advisory: analyzer failures and uncovered semantic changes are reported as warnings instead of blocking release planning.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #523](https://github.com/monochange/monochange/pull/523) _Introduced in:_ [`e7fcb04`](https://github.com/monochange/monochange/commit/e7fcb04f9b80b9e1578a8bb4801fde80e59aec18) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63) _Closed issues:_ [#249](https://github.com/monochange/monochange/issues/249)

#### Add dist profile and ring TLS backend for binary size reduction

_Packages:_ _monochange_, _monochange_core_, _monochange_forgejo_, _monochange_gitea_, _monochange_github_, _monochange_gitlab_, _monochange_hosting_, _monochange_publish_, _monochange_telemetry_, _monochange_test_helpers_

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

_Packages:_ _monochange_, _monochange_config_, _monochange_core_, _monochange_forgejo_, _monochange_gitea_, _monochange_github_, _monochange_gitlab_, _monochange_hosting_

Release commit and release request steps now support a `stage_all` input/config field that defaults to `false`. When enabled, the release commit stages every non-ignored working tree change, so generated lockfile updates like `pnpm-lock.yaml` can be included alongside configured release manifests and changelogs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #520](https://github.com/monochange/monochange/pull/520) _Introduced in:_ [`035dcb3`](https://github.com/monochange/monochange/commit/035dcb345cca8586440451836fa06fb631596c20) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

#### add missing crate metadata and align READMEs with badge template

_Packages:_ _monochange_analysis_, _monochange_lint_, _monochange_linting_, _monochange_test_helpers_

- Add `keywords` to `monochange_analysis`, `monochange_lint`, and `monochange_linting`
- Add `authors`, `categories`, `homepage`, `readme`, `rust-version`, and `keywords` to `monochange_test_helpers`
- Update `monochange_lint`, `monochange_linting`, and `monochange_test_helpers` READMEs to use the badge-row template consistent with other published crates

No API changes. crates.io metadata and documentation only.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #512](https://github.com/monochange/monochange/pull/512) _Introduced in:_ [`f7bc995`](https://github.com/monochange/monochange/commit/f7bc9950aaa58983c2d9b3d53ec1a942debc263d) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

#### Add prerelease mode

_Packages:_ _monochange_changelog_, _monochange_lint_, _monochange_publish_

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

#### Remove core test helper dependency

_Packages:_ _monochange_core_

Remove the monochange_core test dependency on monochange_test_helpers so package publish ordering no longer sees a development dependency cycle between the two crates.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #525](https://github.com/monochange/monochange/pull/525) _Introduced in:_ [`95effe5`](https://github.com/monochange/monochange/commit/95effe5a2ecb4e269a4cdfb1331bd8818a616ca8)

#### Include member changelogs in grouped provider release notes

_Packages:_ _monochange_github_, _monochange_hosting_

Provider release notes now supplement filtered or empty group changelog bodies with member package changelog entries, so grouped releases do not publish the "No group-facing notes" fallback when member packages have release notes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #528](https://github.com/monochange/monochange/pull/528) _Introduced in:_ [`3e58d5a`](https://github.com/monochange/monochange/commit/3e58d5a0c35d55896f3b2c489c55b6cbd069c1e9)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

Grouped release for `main`.

### 📝 Changed

- No group-facing notes were recorded for this release. Member packages were updated as part of the synchronized group `main` version, but their changes are not configured for inclusion in this changelog.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

Grouped release for `main`.

### 🚀 Feature

_Packages:_ _main_

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🐛 Fixed

_Packages:_ _monochange_

#### Group CLI help options by source

Command help now separates generated release flags, configured command inputs, and global flags into distinct headings so users can see where each option originates.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #480](https://github.com/monochange/monochange/pull/480) _Introduced in:_ [`dd01099`](https://github.com/monochange/monochange/commit/dd010996124d8af507ca6bfebb8c32486783fc19) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#475](https://github.com/monochange/monochange/issues/475)

_Packages:_ _monochange_

#### Enable readable multi-step command progress output in CI

This is done while disabling animated spinners in CI logs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #487](https://github.com/monochange/monochange/pull/487) _Introduced in:_ [`e845d1e`](https://github.com/monochange/monochange/commit/e845d1e5212e0d4c1cd3f58d8e4ab9c09cd3fff5) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

_Packages:_ _monochange_

#### Improve custom command argument errors

Unexpected arguments passed to configured CLI commands now show a focused diagnostic with the explicit usage, the `monochange.toml` section to edit, and an example input definition.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #464](https://github.com/monochange/monochange/pull/464) _Introduced in:_ [`b869604`](https://github.com/monochange/monochange/commit/b86960446661884e9732616249232a4aa5e929b3) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

_Packages:_ _monochange_

#### Support release records in shallow checkouts

Fix release record discovery for shallow checkouts when the parent commit is unavailable.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #504](https://github.com/monochange/monochange/pull/504) _Introduced in:_ [`b79accf`](https://github.com/monochange/monochange/commit/b79accfd3d11bbaab94fa8c8b508421615d9029e)

_Packages:_ _monochange_

#### Fix command input references in CLI step conditions

Allow `when` conditions to read command-level inputs while preserving step-level input overrides, so release automation can gate commit and pull request steps on `--commit` and `--create-pr`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #488](https://github.com/monochange/monochange/pull/488) _Introduced in:_ [`f4d96b0`](https://github.com/monochange/monochange/commit/f4d96b0d9eddf21745088a5a95f5c3c69f41b1f5) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🔒 Security

_Packages:_ _monochange_

#### Refresh Ubuntu runner space

Refresh the Ubuntu devenv cache namespace and reclaim runner disk space before the highest-space CI jobs install Nix dependencies.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #506](https://github.com/monochange/monochange/pull/506) _Introduced in:_ [`4a7ad19`](https://github.com/monochange/monochange/commit/4a7ad19c93cba4ba53930ad7a2b216bebc9f9a0b)

### 🧪 Testing

_Packages:_ _monochange_

#### Fix flaky `reuses_prepared_release_artifact_for_versions` test

The `execute_cli_command_with_options_reuses_prepared_release_artifact_for_versions` test (and the related `plans_publish_rate_limits_from_prepared_release_artifact` and `reports_invalid_versions_output_formats` tests) operated on the real repository workspace root without holding the `TEST_ENV_LOCK`. When other test threads modified workspace files concurrently, the `git status` snapshot captured at artifact save time could differ from the snapshot taken at validation time, causing an intermittent "workspace status no longer matches the saved prepared release" error.

All three tests now acquire `TEST_ENV_LOCK` before reading the workspace, serialising them against other tests that modify git state.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #458](https://github.com/monochange/monochange/pull/458) _Introduced in:_ [`a78046a`](https://github.com/monochange/monochange/commit/a78046a1803c136426770d4fb2e6a8928e844e19) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

Grouped release for `main`.

### 🚀 Feature

_Packages:_ _main_

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

Grouped release for `main`.

### 🚀 Feature

_Packages:_ _monochange_

#### Add persistent deduplication index and content-hash fast path for release records

Introduce a JSONL index at `.monochange/local/release-index.jsonl` that survives across CLI invocations, eliminating repeated directory scans when checking for duplicate release records. A fast path in `validate_release_record_file` now compares the `releaseTargets` identity of an existing file against the manifest before rebuilding the full `ReleaseRecord`, skipping unnecessary I/O when the targets match.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #435](https://github.com/monochange/monochange/pull/435) _Introduced in:_ [`b906cab`](https://github.com/monochange/monochange/commit/b906cab608c84b30bfd429298892a80766221ef6) _Closed issues:_ [#430](https://github.com/monochange/monochange/issues/430)

_Packages:_ _monochange_

#### Sort release targets and hash identity fields directly

`release_targets_hash` now sorts targets by `(id, kind, version)` before hashing and only feeds identity fields (`id`, `kind`, `version`) into the hasher. Operational flags (`tag`, `release`, `tag_name`, `version_format`, `members`) are excluded from the hash so that path identity matches release identity.

`ReleasePaths::from_manifest` computes the hash directly from the manifest without building the intermediate `ReleaseRecord`, and `write_release_record_file` now checks file existence before doing any expensive work.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #431](https://github.com/monochange/monochange/pull/431) _Introduced in:_ [`88a6e4c`](https://github.com/monochange/monochange/commit/88a6e4cea075da0b1745cd24a0a150ebea99bb83) _Closed issues:_ [#430](https://github.com/monochange/monochange/issues/430)

### 📖 Documentation

_Packages:_ _monochange_

#### Document `CommitRelease.update_release_json` option

Add comprehensive documentation for the `update_release_json` step-level input on `CommitRelease`:

- Document the input in the `CommitRelease` CLI step reference with type, default, and description
- Explain semantic JSON comparison (formatting-only differences such as indentation or key ordering are ignored)
- Add a new composition example showing how to combine `dprint fmt` formatting with `CommitRelease` using `update_release_json = true`
- Add a new common-mistake entry about running formatters between `PrepareRelease` and `CommitRelease` without setting the input
- Document the field in the configuration guide's workflow variables section
- Regenerate JSON Schema assets to include the new `update_release_json` field in `CommitRelease` step definitions

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #443](https://github.com/monochange/monochange/pull/443) _Introduced in:_ [`4b8cc5a`](https://github.com/monochange/monochange/commit/4b8cc5a25644ab3623177c08bd7904c649ea67a0)

### 📦 Other

_Packages:_ _monochange_

#### Update release commands to regenerate JSON schemas

Before `dprint fmt`, both commands now regenerate JSON schemas from schemars-annotated types so that committed schema assets stay in sync with the source code.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #442](https://github.com/monochange/monochange/pull/442) _Introduced in:_ [`66260c9`](https://github.com/monochange/monochange/commit/66260c9ff4880b73725807ada2ff67ed24c3096a)

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

Grouped release for `main`.

### 🐛 Fixed

_Packages:_ _main_

#### Remove grouped release member summaries

Grouped release notes no longer include generated changed or synchronized member lists, keeping the release note summary focused on the group release itself.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #395](https://github.com/monochange/monochange/pull/395) _Introduced in:_ [`2d012ff`](https://github.com/monochange/monochange/commit/2d012ff900a612f4aed6e4d7034c8c876f50aeae) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

_Packages:_ _monochange_

#### Preserve publish batch dependency order

Carry prior packages into later publish-plan batches so dependency-ordered publish requests remain available when registry rate limits split a release into multiple jobs.

This fixes publish plans for releases that are split by registry rate limits. Dependent packages now continue to see their earlier dependency-ordered predecessors in later publish jobs instead of publishing before required package versions are available.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #389](https://github.com/monochange/monochange/pull/389) _Introduced in:_ [`12d3582`](https://github.com/monochange/monochange/commit/12d35826c3b0a8768bbf05c82b1e999a0e9ca30a) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

_Packages:_ _monochange_

#### Remove indexing/slicing lint allowances

Remove crate-level `clippy::indexing_slicing` allowances and replace production indexing/slicing call sites with safe accessors.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #414](https://github.com/monochange/monochange/pull/414) _Introduced in:_ [`4d06f06`](https://github.com/monochange/monochange/commit/4d06f0695edb280b4dc7ab661cc69449674fe38e)

### 🧪 Testing

_Packages:_ _monochange_

#### Improve readability of multiline JSON snapshots

Redact multiline string fields inside JSON snapshots and assert their contents separately so release-planning test snapshots remain readable without escaped newline sequences.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #398](https://github.com/monochange/monochange/pull/398) _Introduced in:_ [`458b671`](https://github.com/monochange/monochange/commit/458b671252f98a25628cd08a497792149370386d)

_Packages:_ _monochange_

#### Redact schema crate version in snapshot to survive release bumps

Stop hardcoding `monochange_schema` crate version in integration test assertions. Use insta redaction for `schemaCrateVersion` in the schema asset inventory snapshot, and read the expected version from the crate's `Cargo.toml` at runtime.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #406](https://github.com/monochange/monochange/pull/406) _Introduced in:_ [`660d20a`](https://github.com/monochange/monochange/commit/660d20aebadae1096c3f4ddf1d24531c534ee5d4)

## [0.3.4](https://github.com/monochange/monochange/releases/tag/v0.3.4) (2026-05-06)

Grouped release for `main`.

Changed members: @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-arm64-msvc, @monochange/cli-win32-x64-msvc, @monochange/skill, monochange, monochange_analysis, monochange_cargo, monochange_config, monochange_core, monochange_dart, monochange_deno, monochange_ecmascript, monochange_gitea, monochange_github, monochange_gitlab, monochange_go, monochange_graph, monochange_hosting, monochange_lint, monochange_lint_testing, monochange_linting, monochange_npm, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers

### 🐛 Fixed

_Packages:_ _main_

#### Preserve publish batch dependency order

Carry prior packages into later publish-plan batches so dependency-ordered publish requests remain available when registry rate limits split a release into multiple jobs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #389](https://github.com/monochange/monochange/pull/389) _Introduced in:_ [`12d3582`](https://github.com/monochange/monochange/commit/12d35826c3b0a8768bbf05c82b1e999a0e9ca30a)

_Packages:_ _main_

#### Use npm for trusted npm publishing

Route trusted npm publishes through the npm CLI even in pnpm-managed workspaces so npm's OIDC trusted publishing flow can exchange the GitHub Actions identity for a short-lived publish credential. The release workflow also relies on devenv environment cleaning directly instead of the removed `strip:env` wrapper.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #388](https://github.com/monochange/monochange/pull/388) _Introduced in:_ [`72773bc`](https://github.com/monochange/monochange/commit/72773bc438167b55c26bb7c3f5dd9d7a21c99084)

## [0.3.3](https://github.com/monochange/monochange/releases/tag/v0.3.3) (2026-05-06)

Grouped release for `main`.

Changed members: @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-arm64-msvc, @monochange/cli-win32-x64-msvc, @monochange/skill, monochange, monochange_analysis, monochange_cargo, monochange_config, monochange_core, monochange_dart, monochange_deno, monochange_ecmascript, monochange_gitea, monochange_github, monochange_gitlab, monochange_go, monochange_graph, monochange_hosting, monochange_lint, monochange_lint_testing, monochange_linting, monochange_npm, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers

### 🐛 Fixed

_Packages:_ _main_

#### preserve GitHub OIDC environment variables in devenv

The development environment's `devenv.yaml` now keeps the GitHub Actions and OIDC identity variables that monochange needs to detect trusted publishing when running inside `devenv shell`. Previously, `strip: env` removed these variables and caused built-in publishing to fail with "No supported CI provider identity was detected."

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #386](https://github.com/monochange/monochange/pull/386) _Introduced in:_ [`fd1a798`](https://github.com/monochange/monochange/commit/fd1a798e57234fc465c33537077ec6acf0a47db8)

## [0.3.2](https://github.com/monochange/monochange/releases/tag/v0.3.2) (2026-05-06)

Grouped release for `main`.

Changed members: @monochange/cli, monochange, monochange_github

Synchronized members: @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-arm64-msvc, @monochange/cli-win32-x64-msvc, @monochange/skill, monochange_analysis, monochange_cargo, monochange_config, monochange_core, monochange_dart, monochange_deno, monochange_ecmascript, monochange_gitea, monochange_gitlab, monochange_go, monochange_graph, monochange_hosting, monochange_lint, monochange_lint_testing, monochange_linting, monochange_npm, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers

### 🐛 Fixed

_Packages:_ _monochange_

#### Show retarget release progress

`mc repair-release` now updates progress while retargeting a release so long-running provider and git ref updates show the active sub-step instead of a static spinner.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #378](https://github.com/monochange/monochange/pull/378) _Introduced in:_ [`0c381ee`](https://github.com/monochange/monochange/commit/0c381ee4ae199ab02243e455b04002f42bc19305)

## [0.3.1](https://github.com/monochange/monochange/releases/tag/v0.3.1) (2026-05-05)

Grouped release for `main`.

Changed members: @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-arm64-msvc, @monochange/cli-win32-x64-msvc, @monochange/skill, monochange, monochange_analysis, monochange_cargo, monochange_config, monochange_core, monochange_dart, monochange_deno, monochange_ecmascript, monochange_gitea, monochange_github, monochange_gitlab, monochange_go, monochange_graph, monochange_hosting, monochange_lint, monochange_lint_testing, monochange_linting, monochange_npm, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers

### 🐛 Fixed

_Packages:_ _main_

#### Preserve rendered changelog metadata in release records

Release records now store full changelog metadata so publish flows reconstructed from git history can use the rendered release notes instead of falling back to minimal release bodies.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #356](https://github.com/monochange/monochange/pull/356) _Introduced in:_ [`6f38c00`](https://github.com/monochange/monochange/commit/6f38c003a77fcc4a95e33ae1c344340bbcce1017)

_Packages:_ _main_

#### Preserve configured changelog sections for scalar change types

Configured changelog types now take precedence over scalar bump names so generated release notes retain their intended sections. Local telemetry JSONL writes now append complete event lines to avoid malformed records during concurrent command runs.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #363](https://github.com/monochange/monochange/pull/363) _Introduced in:_ [`8c8c9dc`](https://github.com/monochange/monochange/commit/8c8c9dc98f6a95d2c8a2d55fb986a66c08f29312)

_Packages:_ _main_

#### Filter placeholder publish reports to packages that need action

`mc placeholder-publish` now hides already-published and skipped packages from the default report so dry runs focus on packages that still need placeholder publishing, and real runs focus on packages that were published or failed.

Pass `--show-all` to include the full package report when auditing every selected package.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #372](https://github.com/monochange/monochange/pull/372) _Introduced in:_ [`26f20e6`](https://github.com/monochange/monochange/commit/26f20e6347429e57bc94aea06a40eec81f85c54d)

_Packages:_ _main_

#### Publish packages in dependency order without readiness artifacts

Package publishing now derives release work directly from prepared release or `HEAD` release state, orders internal publish-relevant dependencies before dependents, and rejects publish-relevant dependency cycles while allowing development-only cycles.

The publish order now works like this:

1. Build the selected publish requests from the prepared release or `HEAD` release state.
2. Materialize the workspace dependency graph.
3. Consider only dependencies where **both packages are part of the selected publish set**.
4. Ignore development dependency edges.
5. Topologically sort the publish requests so dependencies are emitted before dependents.

So for a tree like:

```text
core        # no dependencies
utils       # depends on core
api         # depends on utils
app         # depends on core, utils, api
```

the publish order becomes:

```text
core
utils
api
app
```

If multiple packages are independent at the same depth, their order is deterministic by package id, registry, and version.

A package with no selected dependencies is eligible first. A package is not published until all of its selected publish-relevant dependencies have been ordered before it. Dependencies outside the selected publish set do not block ordering. Development-only cycles are ignored. Runtime, build, peer, workspace, and unknown dependency cycles fail before publishing anything, with a cycle diagnostic.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e)

_Packages:_ _main_

#### Make release workspace publishing preserve Cargo verification

`monochange_test_helpers` is now publishable so crates that use the shared helpers in their dev-dependencies can still pass Cargo's normal publish verification. `monochange_core` no longer dev-depends on the helper crate: its integration-style discovery filter coverage now lives in the unpublished `monochange_integration_tests` crate, preventing a dependency cycle between the published core crate and the test helper crate.

Package publishing keeps Cargo verification enabled and still runs JavaScript registry tooling without inherited `LD_LIBRARY_PATH`, preserving PNPM support while avoiding Nix/devenv library-path leakage into system Node.js launchers.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #368](https://github.com/monochange/monochange/pull/368) _Introduced in:_ [`b79eef1`](https://github.com/monochange/monochange/commit/b79eef170a01234b69b2b83c8ebd4ef946a079ac)

_Packages:_ _main_

#### Use `GITHUB_TOKEN` for Git Data API to create verified commits

The `release-pr` workflow now passes `GITHUB_COMMIT_TOKEN` (set to `secrets.GITHUB_TOKEN`) specifically for Git Database API operations (blob, tree, commit creation, and ref updates). This allows GitHub to automatically sign commits with the `web-flow` GPG key, producing verified commits on release pull requests.

The `GH_TOKEN` (PAT) continues to be used for all other GitHub API operations like pull request creation and updates.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #371](https://github.com/monochange/monochange/pull/371) _Introduced in:_ [`3770b48`](https://github.com/monochange/monochange/commit/3770b48bab6b41c80086a0d3e2e4e6a9a7540c39)

### 📦 Other

_Packages:_ _main_

#### Resolve git identity from token for release PR commits

The `release-pr` workflow now queries the GitHub API for the authenticated user's `id`, `login`, and `name`, then constructs the standard GitHub noreply email (`{id}+{login}@users.noreply.github.com`) for `git config user.email`. This replaces the previous hardcoded `github-actions[bot]` identity, so release PR commits are properly attributed to the account that owns the `RELEASE_PR_MERGE_TOKEN`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #367](https://github.com/monochange/monochange/pull/367) _Introduced in:_ [`920bf04`](https://github.com/monochange/monochange/commit/920bf04ba34aa7050e0dc6a9be5c488c9431d085)

_Packages:_ _main_

#### Use the current monochange CLI when publishing release tags

The publish workflow now builds the `mc` binary from the workflow commit before checking out the release tag. Publish jobs still operate on the requested release tag's files and release state, but they execute the current workflow version of `mc` so post-release publishing fixes apply when rerunning publication for an older tag.

The workflow keeps full branch and tag history available after switching to the release tag so publish-time release branch reachability checks still work. The release workflow also dispatches `publish.yml` at the current workflow commit, allowing a fixed publish workflow to publish an older release tag.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #366](https://github.com/monochange/monochange/pull/366) _Introduced in:_ [`9bb5ca9`](https://github.com/monochange/monochange/commit/9bb5ca9ca5315f60a1079a55470f7b77ff8e3ea2) _Related issues:_ [#364](https://github.com/monochange/monochange/issues/364)

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-30)

Grouped release for `main`.

Changed members: monochange, monochange_core, monochange_cargo, monochange_npm, monochange_config, monochange_deno, monochange_dart, monochange_python, monochange_go, monochange_graph, monochange_semver, monochange_telemetry, monochange_github, monochange_gitlab, monochange_gitea, monochange_hosting, monochange_analysis, monochange_lint, @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-x64-msvc, @monochange/cli-win32-arm64-msvc, @monochange/skill

Synchronized members: monochange_ecmascript, monochange_linting, monochange_lint_testing

_Packages:_ _monochange_

#### Add cargo-binstall metadata

Add cargo-binstall metadata so `cargo binstall monochange` can resolve the GitHub release archive layout.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #293](https://github.com/monochange/monochange/pull/293) _Introduced in:_ [`497f8c0`](https://github.com/monochange/monochange/commit/497f8c010a534fcac6e3ed26bb21c220c54e7a5e) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Fix CLI help colors

Fix `--help` (`-h`) color output and unify CLI color palette.

- `mc --help` now emits ANSI colors in terminal emulators, matching `mc help <command>` behavior
- Extract shared `cli_theme` module so clap built-in help and custom `mc help` renderer use identical colors:
  - bright cyan for headers and accents
  - bright white for usage
  - bright yellow for flags and literals
  - bright magenta for placeholders
  - bright green for valid/code snippets
  - bright red for errors
  - bright black (gray) for muted text
- Explicitly opt in to `ColorChoice::Auto` on the `Command` builder
- Preserve plain text output in test and CI modes so existing snapshots stay stable

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #267](https://github.com/monochange/monochange/pull/267) _Introduced in:_ [`370d5a1`](https://github.com/monochange/monochange/commit/370d5a1d4655c14cf4340cec7886ddc8aa7bbd51) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Group CLI help commands consistently

Make `mc -h`, `mc --help`, and `mc help` render the same command overview so users see consistent help no matter which entry point they use.

The overview now separates built-in commands, generated `step:*` commands, and user-defined `monochange.toml` commands. Generated step commands are always listed, and detailed command help includes richer descriptions for step commands such as `step:publish-release`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #348](https://github.com/monochange/monochange/pull/348) _Introduced in:_ [`33e82e4`](https://github.com/monochange/monochange/commit/33e82e4df24e7c0a36af70f7a397bbadbf5ff9dd)

_Packages:_ _monochange_

#### Add colored CLI help

Add beautiful colored CLI help with detailed examples

The `mc help <command>` subcommand now renders detailed, formatted help with bordered headers, colored sections, multiple examples per command, tips, and cross-references. Running `mc help` shows an overview listing all commands. The standard `--help` flags also use ANSI colors via an anstyle theme. All colors respect NO_COLOR and TTY detection.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #265](https://github.com/monochange/monochange/pull/265) _Introduced in:_ [`8890d77`](https://github.com/monochange/monochange/commit/8890d77e8d54f81f8807588192441a3cd46bfbb8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Sync documented workflow commands with generated config

Fix the generated `mc init` configuration so it no longer defines the reserved `[cli.validate]` command, restores the documented provider `release-pr` workflow command, and syncs the repository workflow examples with the config-defined commands documented in the README and guides.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #345](https://github.com/monochange/monochange/pull/345) _Introduced in:_ [`13e594b`](https://github.com/monochange/monochange/commit/13e594b62c751b3d6f2779446314d6d283c7e35b)

_Packages:_ _monochange_

#### Fix binary benchmark changeset fixtures

Update generated binary benchmark changesets to include summary headings so the PR benchmark fixtures pass changeset validation.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Fix release merge blocker workflow

Replace the release PR merge blocker action with an inline shell guard so normal pull requests are not blocked by missing action dependencies.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Default CLI output to markdown

Default output format to markdown with termimad terminal rendering.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #263](https://github.com/monochange/monochange/pull/263) _Introduced in:_ [`020df1f`](https://github.com/monochange/monochange/commit/020df1f2d1bec0d8470fe1f4e734ee31e3e167bf) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Improve migration tools

Add `mc migrate audit` to report legacy release tooling, changelog providers, and CI migration signals before moving a repository to monochange.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #332](https://github.com/monochange/monochange/pull/332) _Introduced in:_ [`3f4c89b`](https://github.com/monochange/monochange/commit/3f4c89bd3813317f6a962c38116c74fb0f83e486) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Related issues:_ [#319](https://github.com/monochange/monochange/issues/319)

_Packages:_ _monochange_

#### Publish CLI npm packages with trusted publishing

monochange's own CLI npm package workflow now publishes without `NODE_AUTH_TOKEN` or `NPM_TOKEN`. The publish job keeps the protected `publisher` environment and `id-token: write` permission so npm can use GitHub OIDC trusted publishing and produce provenance for the CLI packages.

**Before:**

```yaml
- name: publish cli npm packages
  env:
    NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
  run: node scripts/npm/publish-packages.mjs --packages-dir packages
```

**After:**

```yaml
- name: publish cli npm packages
  run: node scripts/npm/publish-packages.mjs --packages-dir packages
```

The publish script rejects long-lived npm token environment variables and verifies it is running from `monochange/monochange`'s `publish.yml` workflow with GitHub Actions OIDC context before invoking `npm publish --provenance`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #330](https://github.com/monochange/monochange/pull/330) _Introduced in:_ [`7b3ebab`](https://github.com/monochange/monochange/commit/7b3ebab32b002e8a48595553685d6aaf72434d61) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#309](https://github.com/monochange/monochange/issues/309)

_Packages:_ _monochange_

#### Add provider trust context detection

The capability model distinguishes trusted-publishing support, CI identity detection, registry-side setup verification, setup automation, and registry-native provenance so future enforcement can avoid overstating unsupported provider or registry combinations.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #331](https://github.com/monochange/monochange/pull/331) _Introduced in:_ [`a9c24e5`](https://github.com/monochange/monochange/commit/a9c24e55bd72678f2a67af8fa470387afe722603) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#313](https://github.com/monochange/monochange/issues/313)

_Packages:_ _monochange_

#### Harden publish planning guards

`mc publish-plan`, `mc publish`, and `mc placeholder-publish` now respect the current workspace publishability rules instead of trusting stale release metadata or exact placeholder versions.

For `mc publish-plan --format json`, cargo batches previously included crates with `publish = false`, and release-record entries could keep npm or other ecosystem packages in the plan even after publishing was disabled.

Now publish batches skip packages that are currently private or excluded in discovery, and they also skip packages whose effective publish settings are disabled in the workspace configuration.

For `mc placeholder-publish --dry-run --format json`, placeholder bootstrap checks previously only looked for the exact `0.0.0` version, so a package that already had `1.0.0` on the registry could still be treated as needing a placeholder release.

Now placeholder planning skips any package that already has **any** version on its registry, and npm `setupUrl` values now point at:

```text
https://www.npmjs.com/package/<package>/access
```

`mc publish-plan` also falls back to the crates.io sparse index when the crates.io API denies package lookups, which keeps rate-limit planning working in CI environments that return `403 Forbidden` from the API endpoint.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #264](https://github.com/monochange/monochange/pull/264) _Introduced in:_ [`e542f69`](https://github.com/monochange/monochange/commit/e542f694e15fe91a778c3a66dae66358fe0053b6) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Add initial publish readiness command

Adds `mc publish-readiness` as a non-mutating preflight command for package registry publishing. The command reads a release record from `--from`, dry-runs registry publish checks for the selected package set, reports ready/already-published/unsupported package states, and can write a JSON readiness artifact with `--output`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #292](https://github.com/monochange/monochange/pull/292) _Introduced in:_ [`63cbbe7`](https://github.com/monochange/monochange/commit/63cbbe7c06b03c0f1ed215a4fc61e0a74b50e1c4) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Attest GitHub release archives

monochange's own GitHub release asset workflow now runs from tag or manual dispatch events instead of draft release creation events. This makes the workflow compatible with GitHub immutable releases, where assets should exist before the release is finalized and draft `release.created` events are not a reliable trigger.

**Before:**

```yaml
on:
  release:
    types: [created]
```

The workflow uploaded CLI archives and checksum files, but did not create first-class GitHub artifact attestations for the uploaded `.tar.gz` and `.zip` archives.

**After:**

```yaml
on:
  push:
    tags:
      - "v*"
  workflow_dispatch:
```

The release asset job now requests the minimum attestation permissions, downloads each uploaded archive back from the release, creates GitHub build-provenance attestations for those archive subjects, and verifies the attestations before triggering downstream package publishing.

Users can verify a published archive with:

```bash
gh attestation verify monochange-x86_64-unknown-linux-gnu-v1.2.3.tar.gz \
  --repo monochange/monochange
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #329](https://github.com/monochange/monochange/pull/329) _Introduced in:_ [`ebc26a2`](https://github.com/monochange/monochange/commit/ebc26a2b23eef84660d079fdb1d8d5ad68d3f20c) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#308](https://github.com/monochange/monochange/issues/308)

_Packages:_ _monochange_

#### Ignore changelog-only updates in affected checks

Release automation now treats configured changelog targets as release metadata instead of as ordinary package source changes. That means changelog-only updates no longer make `mc affected --verify` fail with an uncovered package error, and newly generated release notes are inserted above older release headings so the latest release stays at the top of each changelog.

Configured changelog targets are unchanged:

```toml
[package.core.changelog]
path = "crates/core/changelog.md"
```

Command used by CI and local verification:

```bash
mc affected --format json --verify --changed-paths crates/core/changelog.md
```

**Before (output):**

```json
{
	"status": "failed",
	"affectedPackageIds": ["core"],
	"matchedPaths": ["crates/core/changelog.md"],
	"uncoveredPackageIds": ["core"]
}
```

**After (output):**

```json
{
	"status": "not_required",
	"affectedPackageIds": [],
	"ignoredPaths": ["crates/core/changelog.md"],
	"matchedPaths": [],
	"uncoveredPackageIds": []
}
```

Generated changelog sections also stay in reverse-chronological order:

```md
# Changelog

## [0.3.0] - 2026-04-23

- latest release notes

## [0.2.0] - 2026-03-01

- previous release notes
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #278](https://github.com/monochange/monochange/pull/278) _Introduced in:_ [`61a0593`](https://github.com/monochange/monochange/commit/61a0593264c153d6174beb4124812f5055a194dc) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### Tighten release PR CI guards

The built-in GitHub Actions release automation now treats a commit as a release commit only when `HEAD` itself matches the stored release record. That prevents ordinary commits from skipping `publish:check` just because an older release record exists somewhere in history.

Command used by the workflow:

```bash
mc release-record --from HEAD --format json
```

**Before (workflow behavior):**

```yaml
if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
else
echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
fi
```

Any reachable release record could make CI behave as if the current commit was the release commit.

**After:**

```yaml
resolved_commit="$(jq -r '.resolvedCommit' /tmp/release-record.json)"
record_commit="$(jq -r '.recordCommit' /tmp/release-record.json)"

if [ "$resolved_commit" = "$record_commit" ]; then
echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
else
echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
fi
```

With that guard in place:

- `publish:check` is skipped only for the actual release commit at `HEAD`
- the generated `release.yml` template uses the same detection logic
- the `release-pr` job now runs only on pushes to `main`
- the workflow passes `GH_TOKEN` to `mc release-pr` so the built-in GitHub provider can authenticate without extra wrapper scripting

_Owner:_ Ifiok Jr. _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`8b73540`](https://github.com/monochange/monochange/commit/8b7354011d99194a74450ad6907bcff5978b8e28) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

_Packages:_ _monochange_

#### enforce trusted publishing before registry publish commands

Packages with effective `publish.trusted_publishing = true` now fail before monochange invokes a built-in registry publish command unless the current environment exposes a verifiable CI/OIDC identity.

For GitHub Actions trusted publishing, monochange verifies the configured repository, workflow, optional environment, and `id-token: write` OIDC request variables. npm packages also reject long-lived token variables such as `NPM_TOKEN` and `NODE_AUTH_TOKEN` so trusted publishing cannot silently fall back to token-based publishing.

Use the same package configuration as before:

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.trusted_publishing]
workflow = "publish.yml"
environment = "publisher"
```

Run release publishing from the configured CI workflow, or set `publish.trusted_publishing = false` on an individual package when that package intentionally uses a manual publishing path.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #338](https://github.com/monochange/monochange/pull/338) _Introduced in:_ [`71dc3d0`](https://github.com/monochange/monochange/commit/71dc3d0632403a3a79f07fc58c1e656788a75cbd) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#312](https://github.com/monochange/monochange/issues/312)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

Grouped release for `main`.

Changed members: monochange, monochange_core, monochange_cargo, monochange_npm, monochange_config, monochange_deno, monochange_ecmascript, monochange_dart, monochange_graph, monochange_semver, monochange_github, monochange_gitlab, monochange_gitea, monochange_hosting, monochange_analysis, monochange_lint, monochange_linting, monochange_lint_testing, @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-x64-msvc, @monochange/cli-win32-arm64-msvc, @monochange/skill

### 🚀 Feature

_Packages:_ _monochange_

#### add visual status summary to benchmark CI comment sections

`monochange` benchmark PR comments now show an at-a-glance status summary inside each collapsed `<details>` section, so reviewers can see improvements and regressions without expanding anything.

**Before:**

- benchmark PR comments rendered every fixture table and phase timing table fully expanded
- scrolling to later fixtures required paging through the entire earlier benchmark output
- when sections were collapsed, there was no way to tell if a fixture improved or regressed without expanding it

**After:**

- each benchmark fixture renders as a collapsed section with a summary line showing emoji indicators
- per-command status: 🟢 improved · 🔴 regressed · ⚪ flat (for hyperfine tables with relative data)
- phase-level status: 🟢 phases improved · 🔴 phases regressed (for tables without relative comparison data)
- 🚨 over budget shown when any phase exceeds its configured budget
- reviewers can expand only the fixture tables they need while keeping the rest of the comment compact

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #258](https://github.com/monochange/monochange/pull/258) _Introduced in:_ [`d1fa746`](https://github.com/monochange/monochange/commit/d1fa7467bb8bc207939cbf10a907c5dc8fe725d4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

_Packages:_ _monochange_

#### add built-in package publishing and placeholder bootstrap commands

monochange can now publish package artifacts directly from its own release state instead of leaving registry publication entirely to external scripts.

**Before:**

```bash
mc release --dry-run --format json
mc publish-release --dry-run --format json
```

`mc publish-release` only handled hosted/provider releases such as GitHub releases. Package registry publication still had to be wired separately.

**After:**

```bash
mc placeholder-publish --format text
mc publish --format text
mc publish-release --format json
```

- `mc placeholder-publish` checks each built-in package registry and publishes a placeholder `0.0.0` package only when the package does not exist yet
- `mc publish` reads monochange release state and runs the built-in registry publish flow for supported public registries
- npm workspaces that use `pnpm` now publish with `pnpm publish`, and trusted-publishing setup runs through `pnpm exec npm trust ...`

**Before (`mc release --dry-run --format json`):**

```json
{
	"manifest": {
		"releaseTargets": [{ "id": "core", "version": "1.2.3" }]
	}
}
```

**After:**

```json
{
	"manifest": {
		"releaseTargets": [{ "id": "core", "version": "1.2.3" }],
		"packagePublications": [
			{
				"package": "core",
				"ecosystem": "cargo",
				"registry": "crates_io",
				"mode": "builtin",
				"version": "1.2.3"
			}
		]
	}
}
```

Built-in publishing also reports trusted-publishing status in text, markdown, and JSON output, including manual setup URLs when a registry still needs trust configured.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/monochange/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### 🐛 Fixed

_Packages:_ _monochange_

#### align publish rate-limit plans with pending registry work

`mc publish`, `mc placeholder-publish`, and `mc publish-plan` now count only the package versions that are still missing from their registries when they build `publishRateLimits` output.

**Before:**

```bash
mc publish --dry-run --format json
mc placeholder-publish --dry-run --format json
mc publish-plan --format json
```

If some selected package versions were already present in their registries, the rate-limit report could still count them as pending work and recommend extra batches even though the publish command would skip them.

**After:**

```bash
mc publish --dry-run --format json
mc placeholder-publish --dry-run --format json
mc publish-plan --format json
```

The `publishRateLimits` report now shrinks automatically on reruns, partial publishes, and placeholder bootstrap flows where some packages already exist. That keeps advisory warnings, optional enforcement, and CI batch plans aligned with the actual work left to publish.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #240](https://github.com/monochange/monochange/pull/240) _Introduced in:_ [`63fbe0d`](https://github.com/monochange/monochange/commit/63fbe0de9825f3139386b7a25cf4821156813301) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

_Packages:_ _monochange_

#### make manual trusted-publishing guidance more actionable

Improves CLI guidance for registries that still require manual trusted-publishing setup.

**Updated behavior:**

- manual trusted-publishing messages now point users to open the registry setup URL and match repository, workflow, and environment to the current GitHub context
- package-publish text and markdown output now include a concrete next step telling users to finish registry setup and rerun `mc publish`
- built-in publish prerequisite failures now tell users to complete registry setup and rerun the publish command

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #216](https://github.com/monochange/monochange/pull/216) _Introduced in:_ [`3ffb516`](https://github.com/monochange/monochange/commit/3ffb5165d643371be3315edf715a80b04f277144) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

_Packages:_ _monochange_

#### improve trusted-publishing preflight diagnostics for manual registries

Improves trusted-publishing diagnostics for registries that still require manual setup.

**Updated behavior:**

- built-in publish preflight now validates the GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- manual-registry guidance now surfaces the resolved repository, workflow, and environment when monochange can infer them
- manual-registry errors now explain when the GitHub context is incomplete and point to the exact `publish.trusted_publishing.*` field that still needs configuration

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #218](https://github.com/monochange/monochange/pull/218) _Introduced in:_ [`85bc41f`](https://github.com/monochange/monochange/commit/85bc41f72766a34981e25cf1ad73442e9e80c267) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### 🧪 Testing

_Packages:_ _monochange_

#### Fix CI race condition where tests that spawn `git` could fail under parallel `cargo llvm-cov` execution because skill command tests temporarily replace `PATH`. Capture the original `PATH` at process start and pass it explicitly to every git subprocess spawned by test helpers. Also reorder coverage job so Codecov uploads always complete before the patch threshold gate fails.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #262](https://github.com/monochange/monochange/pull/262) _Introduced in:_ [`184ab4f`](https://github.com/monochange/monochange/commit/184ab4fab3cf96f58b14f905a66511c6d0a469aa) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

_Packages:_ _monochange_

#### add fixture-first integration coverage for manual trust diagnostics

Adds fixture-based CLI coverage for manual-registry trusted-publishing diagnostics.

The new integration tests cover:

- resolved GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- missing workflow configuration guidance when monochange cannot resolve the GitHub workflow yet
- placeholder-publish dry-run output in both text and JSON formats

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #221](https://github.com/monochange/monochange/pull/221) _Introduced in:_ [`c7a0209`](https://github.com/monochange/monochange/commit/c7a0209392b81f70b5d51b0b777db40487b8ac29) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

_Packages:_ _monochange_

#### add trusted-publishing messaging test coverage

Adds regression coverage for trusted-publishing messaging in the `monochange` CLI and package-publish reporting.

The new tests cover:

- manual registry setup guidance rendering in text and markdown output
- preservation of explicit trusted-publishing context in manual-action outcomes

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #215](https://github.com/monochange/monochange/pull/215) _Introduced in:_ [`36c1d4e`](https://github.com/monochange/monochange/commit/36c1d4ec3c2daa675c233e388e161f339a77b6c2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

## [0.1.0](https://github.com/monochange/monochange/releases/tag/v0.1.0) (2026-04-13)

Grouped release for `main`.

Changed members: monochange, monochange_core, monochange_cargo, monochange_npm, monochange_config, monochange_deno, monochange_dart, monochange_graph, monochange_semver, monochange_github, monochange_gitlab, monochange_gitea, monochange_hosting

### 💥 Breaking Change

_Packages:_ _main_

#### 🚀 Initial public release of monochange

**monochange** is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems. It is designed from the ground up to support the modern, AI-driven development landscape where agents and automation play a central role in software delivery.

##### What is monochange?

In today's agent-driven development environment, managing releases across diverse package ecosystems (Rust, JavaScript/TypeScript, Dart, Python, etc.) becomes increasingly complex. monochange provides a unified, programmatic interface for:

- **Change tracking**: Structured changesets that capture intent across multiple packages
- **Release planning**: Automated versioning and changelog generation
- **Multi-ecosystem support**: Native handling of Cargo, NPM, Dart, Deno, and more
- **CI/CD integration**: Seamless workflows for Gitea, GitHub, and GitLab
- **Graph-based dependency analysis**: Understanding package relationships across your monorepo

##### Why monochange matters for AI-driven workflows

As development teams increasingly rely on AI agents to generate code, manage dependencies, and orchestrate releases, monochange provides the structured foundation these agents need to operate effectively. It transforms release management from a manual, error-prone process into a deterministic, automatable workflow.

##### What's included in this release

This first release includes:

- Core changeset management engine
- Multi-ecosystem package detection and versioning
- Hosting provider integrations (Gitea, GitHub, GitLab)
- Semantic versioning utilities
- Configurable release workflows
- CLI tooling for validation and release orchestration

For complete feature details, architecture overview, and usage examples, see the [documentation](https://docs.rs/monochange).

_Owner:_ Ifiok Jr. _Introduced in:_ [`4542b5a`](https://github.com/monochange/monochange/commit/4542b5aee8b63a86c7ffc0ea9436090162a18056)
