# Changelog

## Unreleased

### 📝 Changed

- Rewrote the skill package around the current monochange CLI/tool harness.
- Documented verified built-in commands, step commands, MCP tools, user-defined command behavior, and all current CLI step types.
- Replaced obsolete examples with current `monochange.toml`, changeset, release-preview, and publishing workflow examples.

## [0.8.1](https://github.com/monochange/monochange/releases/tag/v0.8.1) (2026-06-09)

### 🐛 Fixed

#### Add manifest-repository lint rule across all ecosystems

New lint rule that enforces the `repository` field in manifest files (Cargo.toml, pubspec.yaml, package.json) to point to the correct monorepo subdirectory. All rules are Off by default in every preset.

For Cargo, the `cargo/manifest-repository` rule resolves `repository = { workspace = true }` against the root manifest's `workspace.package.repository` (falling back to `package.repository`) and reports a mismatch with an autofix. Set `allow_workspace_inheritance = true` to skip workspace-inherited values instead of resolving them.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #612](https://github.com/monochange/monochange/pull/612)

#### Add custom `version_format` tag templates for package and group release identities

`primary` and `namespaced` continue to work as presets, while custom formats such as `{{ ecosystem }}/{{ name }}/v{{ version }}` can use `{{ name }}`, `{{ version }}`, and `{{ ecosystem }}`. Custom formats must include `{{ version }}`, render valid Git tag names, and avoid collisions with other release owners.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #613](https://github.com/monochange/monochange/pull/613)

## [0.8.0](https://github.com/monochange/monochange/releases/tag/v0.8.0) (2026-06-04)

### 🐛 Fixed

#### Update package documentation for the nested CLI command API

Updated generated package documentation, skill guidance, provider-facing examples, and release-record schema fixture text to refer to the new `monochange step <name>` and `monochange run <name>` command paths where those packages expose or document monochange CLI workflows.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #597](https://github.com/monochange/monochange/pull/597) · _Related issues:_ [#35](https://github.com/monochange/monochange/issues/35)

#### Refresh documentation command examples

Update documentation, package readmes, and generated skill command inventory so examples use the current CLI shape: `monochange versions` for dependency synchronization, `monochange step <name>` for built-in steps, and `monochange run <name>` for configured workflows.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #600](https://github.com/monochange/monochange/pull/600)

#### Add normalized CLI snapshots

Add a `monochange_snapshot` crate for normalized command-surface snapshots and expose `mc snapshot` plus the global `--snapshot` flag. The snapshot output gives agents and CI a structured view of supported commands, options, arguments, standard entrypoints, and extractor provenance.

For example, a CLI can produce a normalized snapshot with a stable schema version and extractor provenance:

```json
{
	"schema_version": "0.1",
	"kind": "cli-surface",
	"tool": {
		"name": "mc",
		"version": "0.7.0"
	},
	"provenance": {
		"extractor": "clap",
		"confidence": "high"
	},
	"commands": [
		{
			"path": ["snapshot"],
			"max_bump": "major",
			"hidden": false
		}
	]
}
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #593](https://github.com/monochange/monochange/pull/593)

#### Add explicit versions list and sync commands

Added `monochange versions list` for flat package/group version inventory and `monochange versions sync` for the existing dependency synchronization behavior. The legacy bare `monochange versions` form still works but now warns that it is deprecated.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #603](https://github.com/monochange/monochange/pull/603)

## [0.7.0](https://github.com/monochange/monochange/releases/tag/v0.7.0) (2026-06-03)

### 🐛 Fixed

#### Improve API classification followups

Add advisory validation guidance, public dependency propagation, precise ECMAScript function signatures, and Dart API snapshot support.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #586](https://github.com/monochange/monochange/pull/586)

#### Add API change classification

Add `mc change classify` to classify API-impacting semantic changes and recommend package bumps in markdown or JSON output.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #584](https://github.com/monochange/monochange/pull/584)

## [0.6.8](https://github.com/monochange/monochange/releases/tag/v0.6.8) (2026-05-31)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.6.8 as part of group `main`.

## [0.6.7](https://github.com/monochange/monochange/releases/tag/v0.6.7) (2026-05-30)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.6.7 as part of group `main`.

## [0.6.6](https://github.com/monochange/monochange/releases/tag/v0.6.6) (2026-05-29)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.6.6 as part of group `main`.

## [0.6.5](https://github.com/monochange/monochange/releases/tag/v0.6.5) (2026-05-29)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.6.5 as part of group `main`.

## [0.6.4](https://github.com/monochange/monochange/releases/tag/v0.6.4) (2026-05-28)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.6.4 as part of group `main`.

## [0.6.3](https://github.com/monochange/monochange/releases/tag/v0.6.3) (2026-05-28)

### 🐛 Fixed

#### Document `mc versions` command with full ecosystem coverage

Update documentation and skill to accurately describe the `mc versions` command:

- **All ecosystems supported**: Cargo, Dart, Deno, Go, npm, and Python
- **Usage examples**: `--dry-run`, `--format json`, `--strategy exact/caret/compatible`
- **Ecosystem details**: Each adapter's manifest scanning behavior documented

Fix incorrect reference in start-here guide that described `mc versions` as read-only (it actually writes to manifests).

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #552](https://github.com/monochange/monochange/pull/552)

#### Polish `mc versions` command

Rename `mc sync versions` to top-level `mc versions` with plan/apply abstraction, `--format text|json` output, unsupported ecosystem reporting, and snapshot-tested CLI output. Add Criterion benchmark coverage and documentation.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #540](https://github.com/monochange/monochange/pull/540) · _Related issues:_ [#539](https://github.com/monochange/monochange/issues/539)

## [0.6.2](https://github.com/monochange/monochange/releases/tag/v0.6.2) (2026-05-27)

### 🐛 Fixed

#### Refresh documentation audit coverage

Updates documentation, CLI help text, package README content, and packaged skill guidance so the documented command surface matches the current monochange CLI and release workflow behavior.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #546](https://github.com/monochange/monochange/pull/546)

#### Add `mc sync versions` command for internal dependency synchronization

Add a new CLI subcommand `mc sync versions` that synchronizes internal dependency version references across workspace packages to match each package's canonical version.

- **`VersionStrategy` enum** in `monochange_core` controls constraint format: `Default`, `Exact`, `Caret`, `Compatible`.
- **`DependencySyncChange` struct** in `monochange_core` reports what changed (dependency name, section, old value, new value).
- **`sync_internal_dependency_versions()`** in `monochange_dart` scans pubspec.yaml `dependencies`, `dev_dependencies`, and `dependency_overrides` for internal workspace references and computes the target version constraint. Under `resolution: workspace`, `path:` references are converted to versioned constraints.
- **`sync_internal_dependency_versions()`** in `monochange_npm` scans package.json for internal workspace dependencies, skipping `workspace:*` protocol references.
- **CLI subcommand** `mc sync versions [--dry-run] [--strategy <default|exact|caret|compatible>]` orchestrates discovery, version map building, and per-ecosystem sync.
- **`--dry-run`** flag shows what would change without writing files.

Currently supports **Dart** and **npm** ecosystems. Other ecosystems will be added in follow-up changes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #538](https://github.com/monochange/monochange/pull/538) · _Related issues:_ [#536](https://github.com/monochange/monochange/issues/536), [#537](https://github.com/monochange/monochange/issues/537)

## [0.6.1](https://github.com/monochange/monochange/releases/tag/v0.6.1) (2026-05-24)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.6.1 as part of group `main`.

## [0.6.0](https://github.com/monochange/monochange/releases/tag/v0.6.0) (2026-05-23)

### 🐛 Fixed

#### Add `nix` / `devenv` installation instructions

Mention the `ifiokjr/nixpkgs` flake overlay.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #519](https://github.com/monochange/monochange/pull/519) _Introduced in:_ [`23bd962`](https://github.com/monochange/monochange/commit/23bd962434001e4293abdd8e9d33cf185cab3221) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63)

#### add semantic semver guardrails to release planning

Release planning now folds semantic analyzer evidence into compatibility assessments so public API and export diffs can raise the planned bump during previews. The guardrail is advisory: analyzer failures and uncovered semantic changes are reported as warnings instead of blocking release planning.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #523](https://github.com/monochange/monochange/pull/523) _Introduced in:_ [`e7fcb04`](https://github.com/monochange/monochange/commit/e7fcb04f9b80b9e1578a8bb4801fde80e59aec18) _Last updated in:_ [`88b520e`](https://github.com/monochange/monochange/commit/88b520ec51b76c79348595abc66a573761da4d63) _Closed issues:_ [#249](https://github.com/monochange/monochange/issues/249)

## [0.5.1](https://github.com/monochange/monochange/releases/tag/v0.5.1) (2026-05-15)

### 📝 Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.5.1 as part of group `main`.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

### 💥 Breaking Change

#### require CLI steps to opt in to inherited command inputs

> **Breaking change** — CLI step inputs are now explicit. Command-level inputs no longer automatically appear in every configured CLI step.

A configured step now receives only the inputs listed in that step's `inputs` field. This removes ambiguous behavior where a command-level flag could unexpectedly shadow a step-specific input with the same name.

**Before:** every step implicitly saw all command inputs, even with no step-level `inputs` entry:

```toml
[cli.release]
inputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]
steps = [{ type = "PrepareRelease" }]
```

**After:** inherit command inputs explicitly with the array shorthand:

```toml
[cli.release]
inputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]
steps = [{ type = "PrepareRelease", inputs = ["format"] }]
```

Map overrides still work for fixed or templated step values:

```toml
steps = [
	{ type = "PrepareRelease", inputs = ["format"] },
	{ type = "PublishRelease", inputs = { format = "json", draft = "{{ inputs.draft }}" } },
]
```

Migration path: review custom `[cli.<command>]` definitions and add `inputs = ["name"]` to every step that needs a command-level input. Built-in default CLI commands and generated templates have been updated to declare their inherited inputs explicitly.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #467](https://github.com/monochange/monochange/pull/467) _Introduced in:_ [`ce4712f`](https://github.com/monochange/monochange/commit/ce4712f2890e0636c368b056db756df32f4cf769) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### generate built-in release and validation step commands

> **Breaking change** — several hardcoded top-level commands now live under generated immutable `mc step:*` command names.

The release-record, publish-readiness, tag-release, placeholder-publish, and validation operations now share the generated step-command path used by the rest of the CLI step catalog. This keeps their help, schema metadata, docs, and automation examples consistent with configured workflow steps while preserving the distinction between binary commands, generated step commands, and optional user-defined `[cli.*]` workflow aliases.

**Before:** scripts could call these hardcoded top-level commands directly:

```bash
mc validate
mc release-record --from HEAD --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc tag-release --from HEAD
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
```

**After:** call the generated step command names instead:

```bash
mc step:validate
mc step:release-record --from HEAD --format json
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:tag-release --from HEAD
mc step:placeholder-publish --from HEAD --output .monochange/bootstrap-result.json
```

`mc init` also writes a smaller starter configuration. It no longer seeds redundant generated `[cli.*]` aliases for commands that already exist as immutable step commands.

**Before:** starter configs included workflow aliases for generated behavior:

```toml
[cli.validate]
steps = [{ type = "Validate" }]
```

**After:** starter configs rely on the generated command directly and reserve `[cli.*]` for repository-specific chains, custom inputs, or shell `Command` steps:

```bash
mc step:validate
```

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #479](https://github.com/monochange/monochange/pull/479) _Introduced in:_ [`d9adff8`](https://github.com/monochange/monochange/commit/d9adff8fb396df908e335d2a6688aa729abb5f4d) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#476](https://github.com/monochange/monochange/issues/476)

### 🚀 Feature

#### Configurable publish-order dependency fields

Add configurable ecosystem-specific dependency fields for package publish ordering across npm, Cargo, Deno, Dart/Flutter, Python, and Go.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #472](https://github.com/monochange/monochange/pull/472) _Introduced in:_ [`0d9cf46`](https://github.com/monochange/monochange/commit/0d9cf461a05057b61efa987d361ebd27d800dbdb) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#465](https://github.com/monochange/monochange/issues/465)

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🐛 Fixed

#### Add interactive CLI command wizard

Added `mc command`, an interactive dashboard for adding and editing `[cli.<name>]` commands in `monochange.toml`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #471](https://github.com/monochange/monochange/pull/471) _Introduced in:_ [`fea471c`](https://github.com/monochange/monochange/commit/fea471c4b67b618cde51eaacfd4e30742cfb0dc1) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### add release-record migration command

Add `mc migrate release-records` to rewrite persisted release records to the latest schema version, expose the release-record migration helper from core, and update the generated skill command inventory.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #500](https://github.com/monochange/monochange/pull/500) _Introduced in:_ [`bd56420`](https://github.com/monochange/monochange/commit/bd564204b786961371b0ac1bad21071ebe5fe90c) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Rewrite monochange skill guidance

The monochange skill package now documents the current CLI/tool harness, verified built-in commands, step commands, MCP tools, custom `monochange.toml` workflows, and package versioning examples. The command guide also includes a generated inventory checked by `cargo xtask skill commands check` to prevent future CLI drift.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #463](https://github.com/monochange/monochange/pull/463) _Introduced in:_ [`0f3d15c`](https://github.com/monochange/monochange/commit/0f3d15c38b15124a9bb96ed4c73829602e34e838) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### 🧪 Testing

#### Validate generated release commits in PR CI

Pull requests now run release-state test and lint preflights after creating a local release commit, while generated release PRs skip those extra preflights.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #477](https://github.com/monochange/monochange/pull/477) _Introduced in:_ [`a09020f`](https://github.com/monochange/monochange/commit/a09020f9282be207ace6f641b716c3c4004886af) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)
