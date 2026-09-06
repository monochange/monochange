# Changelog

## Unreleased

### 📝 Changed

- Rewrote the skill package around the current monochange CLI/tool harness.
- Documented verified built-in commands, step commands, MCP tools, user-defined command behavior, and all current CLI step types.
- Replaced obsolete examples with current `monochange.toml`, changeset, release-preview, and publishing workflow examples.

## [0.10.1](https://github.com/monochange/monochange/releases/tag/v0.10.1) (2026-09-06)

### 🐛 Fixed

#### refresh docs and validate doc samples in tests

Every crate's crate-level docs (lib.rs doc comments) now come from the same shared mdt block that feeds its readme, so the two surfaces can no longer drift. Crate docs that had fallen behind the code were rewritten: `monochange_analysis` documents the semantic-analyzer architecture, `monochange_lint` documents the real `Linter`/`lint_workspace` API instead of removed entry points, `monochange_linting` carries the authoring guidance, `monochange_graph` documents the current `build_release_plan` signature with `bump_propagation`, and the `monochange_go`/`monochange_python` intros match the actual adapters.

Documentation samples that declare a complete `monochange.toml` are now validated in tests against the real configuration loader, which caught and fixed several stale samples: the removed `[release_notes]`/`change_templates`/`extra_changelog_sections` options were replaced with the current `[changelog]` API, knope-migration samples no longer use the unsupported `dependency` versioned-file syntax, and `monochange step placeholder-publish` invocations dropped the removed `--from`/`--output` flags.

Command references now distinguish `monochange versions sync` from the read-only `versions list` (and mention the deprecation of bare `monochange versions`), the `publish-packages` reference documents `--all`, `--stream-output`, and `--fail-on-duplicate`, the `comment-released-issues` reference documents `--from-ref` and `--auto-close-issues`, the retarget-release reference no longer documents a `format` input the step does not accept, and the knope-migration and `init --provider` claims now match what those commands actually generate. The skill gained the type-scoped `changesets/types/<type>` lint rules from the linting reference.

New doc-sample validation tests in `monochange_integration_tests` (`docs_code_samples.rs`) parse every fenced `toml` sample in the guide through `load_workspace_configuration`, so documentation samples fail CI when the configuration surface changes.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #656](https://github.com/monochange/monochange/pull/656)

## [0.10.0](https://github.com/monochange/monochange/releases/tag/v0.10.0) (2026-09-03)

### 🚀 Feature

#### teach agents to write auditable audience-specific changesets

The Monochange agent skill now explains how to separate developer and user release notes without adding audience metadata to changeset bodies. Agents select a configured type, keep one stream per file, and write prose for that stream's readers.

```markdown
---
app: app_feature
---

# make project lists open faster

Large project lists now appear sooner and remain responsive while more results load.
```

For mobile repositories, the guidance distinguishes a `native` major change that requires a new store binary from an `app_feature` minor change that may use a patch delivery system such as Shorebird. When one implementation matters to both developers and users, agents create two independently reviewable changesets with different types and audience-appropriate prose rather than combining both audiences in one file.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #652](https://github.com/monochange/monochange/pull/652)

#### extract one configured release-note output without preparing a release

The new read-only `monochange notes` command renders the stream and format selected by a named changelog output. It prints one artifact to stdout by default, accepts `--target` when an output covers multiple packages or groups, and writes only an explicitly requested `--file` path.

**Before:** automation had to parse the complete dry-run manifest or prepare configured changelog files to obtain one audience's notes.

```bash
monochange step prepare-release --dry-run --format json
```

**After:** select the configured artifact directly.

```bash
monochange notes --output user --target app
monochange notes --output user --target app --file artifacts/app-release-notes.md
```

Rendering does not update manifests, consume changesets, or write the configured changelog destination. The bundled agent skill documents how to choose stream-specific types, author separate developer and user changesets, validate them, and use extracted notes in reviews, CI, hosted releases, app-store releases, or patch delivery.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #652](https://github.com/monochange/monochange/pull/652)

### 🐛 Fixed

#### add `--format json-min` and guarantee plain text JSON output

Every command that accepts `--format json` (and every cli step `format` input) now guarantees plain-text JSON: no text colors, no background colors, and no other terminal styling leak into the output, even when color support would otherwise be detected. Machine consumers can pipe the output to a JSON parser without stripping escape sequences.

A new `json-min` choice renders the exact same data minified, with no indentation and no whitespace between tokens, which is convenient for piping into `--jq` filters, CI annotations, or log systems that prefer compact payloads.

```bash
# before
monochange run release --dry-run --format json
# → pretty-printed JSON (multi-line, indented)

# after — same data, one compact line
monochange run release --dry-run --format json-min
```

```bash
monochange versions list --format json-min
# {"core":"0.1.0"}
```

`json-min` is accepted anywhere `json` was: `analyze`, `check`, `lint`, `migrate`, `subagents`, `versions list/sync`, and the built-in step inputs (`[cli.*]` command inputs with `type = "choice"`, `choices = ["text", "json", "md"]` now also accept `"json-min"`):

```toml
[cli.release]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "json-min", "markdown"], default = "markdown" },
]
```

Rejecting a JSON format no longer depends on terminal color detection either: styling is applied exclusively in `text` and `markdown` modes, so `NO_COLOR`-style env vars are no longer needed to keep JSON output clean in CI.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #648](https://github.com/monochange/monochange/pull/648) · _Related issues:_ [#2048](https://github.com/monochange/monochange/issues/2048), [#646](https://github.com/monochange/monochange/issues/646), [#652](https://github.com/monochange/monochange/issues/652), [#654](https://github.com/monochange/monochange/issues/654)

#### polish the monochange readme layout and shorten headings

The monochange readme now centers its title, logo, badges, and intro blockquote at the top of the page, uses `<br />` spacing before headings and after every section for more breathing room between sections, and shortens every section heading to one or two Title Case words.

The workspace crate catalog is now a table with one row per crate: the crate name, its crates.io and docs.rs badge links, and a short description, replacing the nested bullet list.

```markdown
# before

## Command and automation matrix

- `monochange`: end-user CLI and orchestration layer for discovery, planning, and CLI-defined release commands.
  - [![Crates.io](…)](…) [![Docs.rs](…)](…)

# after

## Commands

| Crate        | Badges                                  | Description                                                                                     |
| ------------ | --------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `monochange` | [![Crates.io](…)](…) [![Docs.rs](…)](…) | end-user CLI and orchestration layer for discovery, planning, and CLI-defined release commands. |
```

The `Repository development` section is renamed to `Contributing`, and shared documentation blocks were renamed with it (`Quick CLI workflow` becomes `Quick Start`, which also shortens the `monochange --help` long help heading). The regenerated npm `@monochange/cli` README and `@monochange/skill` docs inherit the same table and heading updates through the shared `mdt` blocks.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #651](https://github.com/monochange/monochange/pull/651)

## [0.9.2](https://github.com/monochange/monochange/releases/tag/v0.9.2) (2026-08-29)

<details>
<summary><strong>📖 Documentation</strong></summary>

#### declare how packages and groups propagate bumps to dependents

Release planning now supports per-package and per-group `bump_propagation` in `monochange.toml`. A package or group can declare that its changes are matched by dependents (`inherit`), bounded by a maximum (`bump_propagation_max`), pinned to a fixed floor (`none`/`patch`/`minor`/`major`), or left at the workspace `[defaults].parent_bump`. `monochange check` and release planning honor these declarations, and the JSON schema and the monochange skill documentation describe the new fields.

**Before (only the workspace-wide floor existed; a breaking dependency left dependents at `parent_bump`):**

```toml
[defaults]
parent_bump = "patch"
```

```markdown
---
sdk-core: breaking
---
```

Plan: `sdk-core` → major, but every dependent (app, cli) only → patch, even though a breaking dependency is itself breaking for them.

**After (declare inheritance with a clamp, and a floor on another package):**

```toml
[package."@solana/kit"]
path = "crates/kit"
bump_propagation = "inherit"
bump_propagation_max = "minor"

[package."@solana/leaf"]
path = "crates/leaf"
bump_propagation = "none"
```

```markdown
---
kit: breaking
---
```

Release plan: `kit` → major, `app` → minor (inherit matches breaking, clamped to minor), and nothing releases for the leaf's dependent. Groups can declare their own propagation, which overrides declarations of member packages, and changesets can still author an explicit bump for a dependent with `caused_by`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #643](https://github.com/monochange/monochange/pull/643)

#### add `[defaults].bump_propagation` and most-specific-first precedence

Bump propagation policies now resolve most-specific-first: a package declaration overrides its group declaration, which overrides the new `[defaults].bump_propagation` (with the optional `[defaults].bump_propagation_max` clamp), which overrides the legacy `[defaults].parent_bump` floor.

**Before (only the workspace floor applied to undeclared targets):**

```toml
[defaults]
parent_bump = "major"
```

Every dependent of any changed package had to release major, even when the source's change was a patch.

**After (workspace-wide inherit fallback without redeclaring per package):**

```toml
[defaults]
bump_propagation = "inherit"

[group.kit]
packages = ["kit-core"]

[package.kit-core]
bump_propagation = "inherit"
bump_propagation_max = "minor"
```

Precedence: the most specific declaration wins — the kit-core package clamp (minor) overrides the group's unclamped inherit, which overrides the defaults. Packages and groups with no declaration pick up the defaults inherit; a target can still pin itself with `bump_propagation = "none"`.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #644](https://github.com/monochange/monochange/pull/644) · _Related issues:_ [#643](https://github.com/monochange/monochange/issues/643)

#### accept native TOML booleans and numbers in CLI step inputs

CLI step input overrides in `monochange.toml` no longer need to be strings. Authoring `{ draft = true }` (boolean) and `{ jobs = 4, ratio = 2.5 }` (numbers) in a step `inputs` map now parses, and the JSON Schema for the config accepts all three literal shapes:

- booleans keep their native type through parsing and are stringified to `"true"`/`"false"` when the step runs (unchanged behavior, now covered by tests)
- numbers are coerced to their string form at parse time, so `{ jobs = 4 }` is exactly `{ jobs = "4" }` once the step runs
- input declarations already accepted boolean and number `default` literals; that behavior is now covered by tests and documented
- the generated JSON Schema for step input overrides accepts `string`, `boolean`, and `number` values
- the configuration guide, the Command step reference, and the bundled skill gain an explicit interactive `Command` step example: declare an `interactive` boolean input, pass it to the step, and run the workflow with `--interactive` so the command inherits stdio and owns the terminal

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #642](https://github.com/monochange/monochange/pull/642)

#### refresh documentation and remove stale CLI references

Updated the guide, reference, crate readme, and agent-facing documentation to match the current CLI model:

- configured `[cli.*]` workflows are documented as `monochange run <name>` commands
- the removed `mc` binary alias is no longer referenced as an install option
- stale `monochange <command>` invocations were replaced with the nested `step` or `run` paths
- generated subagent instructions no longer list the `monochange` executable twice
- knope migration guide now shows the built-in regex versioned-file support instead of a manual `sed` fallback
- duplicated `monochange --help` lines and a duplicated registry entry were removed
- prose was tightened and em dashes were replaced with colons or sentence breaks

The monochange skill now prefers the inline changeset type shorthand whenever the intended bump matches the type's default bump, and reserves object syntax for overriding a type's default bump.

_Owner:_ [@ifiokjr](https://github.com/ifiokjr) · _Review:_ [PR #637](https://github.com/monochange/monochange/pull/637) · _Related issues:_ [#637](https://github.com/monochange/monochange/issues/637)

</details>

## [0.9.1](https://github.com/monochange/monochange/releases/tag/v0.9.1) (2026-08-19)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.9.1 as part of group `main`.

## [0.9.0](https://github.com/monochange/monochange/releases/tag/v0.9.0) (2026-08-14)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.9.0 as part of group `main`.

## [0.8.4](https://github.com/monochange/monochange/releases/tag/v0.8.4) (2026-07-11)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.8.4 as part of group `main`.

## [0.8.3](https://github.com/monochange/monochange/releases/tag/v0.8.3) (2026-06-29)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.8.3 as part of group `main`.

## [0.8.2](https://github.com/monochange/monochange/releases/tag/v0.8.2) (2026-06-18)

### Changed

- No package-specific changes were recorded; `@monochange/skill` was updated to 0.8.2 as part of group `main`.

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
