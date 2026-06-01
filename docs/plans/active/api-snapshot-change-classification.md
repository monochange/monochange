# API snapshot change classification workflow

## Status

- Branch: `docs/api-snapshot-change-classification`
- Worktree: `/Users/ifiokjr/.pi/agent/worktrees/root/root/Users/ifiokjr/Developer/projects/monochange/monochange/worktrees/docs-api-snapshot-change-classification`
- State: implemented
- Primary decision: use monochange-owned API snapshot files and diffing, not `cargo-semver-checks`.

## Problem statement

The current agent workflow is too dependent on the agent's subjective reading of a diff:

1. An agent makes a change.
2. The agent looks at the diff.
3. The agent writes a changeset and chooses `patch`, `minor`, or `major`.

This fails in predictable ways:

- Agents tend to choose `patch` or `minor` even when a public API was removed or changed.
- Agents often cannot tell whether a Rust signature change is public or internal.
- Agents often cannot tell whether an npm export or package entrypoint changed in a breaking way.
- monochange already has semantic analysis concepts, but they are not front-and-centre in the workflow.
- The workflow does not produce a small, structured, agent-readable answer to: "what kind of change did I just make?"

The goal is to make the post-change workflow simple enough that it can become a default instruction in the monochange skill:

```text
make change
→ run monochange change classification
→ use the classification report to choose the changeset bump
→ validate that the changeset is compatible with the detected API impact
```

## Goals

- Add a fast, monochange-native API snapshot workflow.
- Avoid `cargo-semver-checks` because previous experience shows it is too slow for this desired agent loop.
- Start with both Cargo and npm ecosystems.
- Reuse and improve existing semantic analyzers instead of introducing a separate external checker.
- Keep the command surface simple for agents.
- Produce both human-readable and machine-readable output.
- Make breaking-change evidence explicit and tied to package ids.
- Make changeset creation safer without making the first version over-strict.
- Leave room for Dart, Python, CLI, JSON schema/config, and other future public-surface analyzers.

## Non-goals for the MVP

- Do not implement complete Rust semver compatibility. The MVP should classify obvious public surface changes from monochange snapshots.
- Do not depend on `cargo-semver-checks`, rustdoc JSON, TypeScript compiler services, or a full npm package build.
- Do not auto-create changesets in the first pass unless explicitly requested by a later implementation phase.
- Do not block all PRs on low-confidence findings at first.
- Do not solve CLI surface diffing in the MVP, but design the data model so CLI can plug in later.
- Do not require committed snapshot baselines in every repository. Snapshots should be generated from git refs by default.

## Existing foundation in the codebase

monochange already has pieces that can become this workflow:

- `crates/monochange_core/src/analysis.rs`
  - `PackageSnapshot`
  - `PackageSnapshotFile`
  - `AnalyzedFileChange`
  - `SemanticAnalyzer`
  - `PackageAnalysisContext`
  - `PackageAnalysisResult`
  - `SemanticChange`
  - `SemanticChangeCategory`
  - `SemanticChangeKind`
  - `DetectionLevel`
- `crates/monochange_analysis/src/lib.rs`
  - Orchestrates ecosystem-specific analyzers across comparison frames.
  - Already registers Cargo and npm analyzers.
  - Builds package snapshots for before/after refs.
- `crates/monochange_cargo/src/analysis.rs`
  - Already parses Rust source with `syn`.
  - Already extracts public Rust symbols and signatures.
  - Already emits semantic changes for added, removed, and modified public API symbols.
- `crates/monochange_ecmascript/src/lib.rs`
  - Already parses ECMAScript/TypeScript with `oxc` and has a legacy scanner fallback.
  - Already extracts exported symbols and signatures.
  - Already diffs added, removed, and modified exported symbols.
- `crates/monochange_npm/src/analysis.rs`
  - Already uses the shared ECMAScript symbol diffing.
  - Already analyzes npm manifest/export/dependency/metadata changes.
- `mc analyze`
  - Already exists as a user-facing command for semantic analysis, but it is not yet shaped around a simple agent workflow and changeset recommendation.

The plan should avoid throwing this away. Instead, promote the current semantic-analysis model into a durable API snapshot + impact classification model.

## Proposed simple workflow

### Agent workflow

After changing code, the agent runs:

```nushell
mc change classify --base origin/main --format markdown
```

For structured consumption:

```nushell
mc change classify --base origin/main --format json
```

Then the agent writes the changeset using the highest required impact from the report.

Before finishing, the agent validates:

```nushell
mc changeset validate --base origin/main --api
```

Suggested skill instruction:

```text
Before creating or finalizing a changeset, run `mc change classify --base origin/main --format markdown`. If it reports a high-confidence breaking API change for a package, use a major changeset for that package or ask the maintainer before overriding. If it reports additive public API only, prefer minor. If it reports no public API impact, choose patch or no changeset based on repository policy and changed paths.
```

### Human workflow

Humans can inspect the same evidence:

```nushell
mc api snapshot --package monochange_core --ecosystem cargo --format json
mc api diff --base origin/main --package monochange_core --format markdown
mc change classify --base origin/main --format markdown
```

### CI workflow, advisory first

Start with advisory CI:

```nushell
mc change classify --base origin/main --format markdown --output api-impact.md
mc changeset validate --base origin/main --api --warn-only
```

Later, strict CI can fail only on high-confidence mismatches:

```nushell
mc changeset validate --base origin/main --api --fail-on high-confidence-breaking
```

## Proposed command APIs

### 1. `mc api snapshot`

Generate a normalized API snapshot for one package at the current checkout.

```nushell
mc api snapshot --package monochange_core --format json
mc api snapshot --package my-npm-package --format json
```

Possible options:

```text
--package <id-or-manifest-name>   Package id/name to snapshot.
--ecosystem <cargo|npm>           Optional override. Usually inferred.
--level <basic|signature|semantic> Detection level. Default: signature.
--output <path>                   Write the snapshot file.
--format <json|pretty-json>       Snapshot serialization format.
```

MVP behavior:

- Cargo: parse source files under the package and emit public Rust items/signatures using the existing `monochange_cargo` analyzer internals.
- npm: parse source files and package manifest/export information using the existing `monochange_npm` + `monochange_ecmascript` internals.
- The command is mainly a debug/development command, not the primary agent command.

### 2. `mc api diff`

Compare API snapshots for a package between two refs.

```nushell
mc api diff --base origin/main --package monochange_core --format markdown
mc api diff --base origin/main --package my-npm-package --format json
```

Possible options:

```text
--base <ref>                      Base ref. Default from config or `origin/main`.
--head <ref>                      Head ref. Default current working tree.
--package <id-or-manifest-name>   Package to diff.
--format <markdown|json>          Output format.
--include-snapshots               Include normalized before/after snapshots in JSON.
```

MVP behavior:

- Generate before snapshot from `--base`.
- Generate after snapshot from `--head` or current working tree.
- Diff normalized API items.
- Convert raw diffs into impact records.

### 3. `mc change classify`

Primary agent-facing command. Analyze affected packages and recommend bumps.

```nushell
mc change classify --base origin/main --format markdown
mc change classify --base origin/main --format json
```

Possible options:

```text
--base <ref>                      Base ref. Default from config or `origin/main`.
--head <ref>                      Head ref. Default current working tree.
--package <id-or-manifest-name>   Limit to one package.
--format <markdown|json>          Output format.
--for-agent                       Short, high-signal report optimized for agents.
--level <basic|signature|semantic> Detection level. Default: signature.
--include-unchanged               Include packages with no public API impact.
```

Example markdown output:

```md
# Change classification

Base: origin/main Head: working tree

## Summary

| Package           | Ecosystem | Suggested bump | Confidence | Why                            |
| ----------------- | --------- | -------------- | ---------- | ------------------------------ |
| monochange_core   | cargo     | major          | high       | removed public function        |
| monochange_npm    | npm       | minor          | high       | added public export            |
| monochange_schema | cargo     | patch          | medium     | no public API changes detected |

## monochange_core

Suggested bump: major Confidence: high

Breaking changes:

- Removed public function `version_plan::from_workspace`
  - file: `src/version_plan.rs`
  - before: `pub fn from_workspace(...) -> MonochangeResult<Self>`

## monochange_npm

Suggested bump: minor Confidence: high

Additive changes:

- Added export `createChangeset`
  - file: `src/index.ts`
  - after: `export function createChangeset(...)`
```

### 4. `mc changeset validate --api`

Validate that pending changesets are compatible with the API impact report.

```nushell
mc changeset validate --base origin/main --api
```

Possible options:

```text
--api                             Enable API impact validation.
--warn-only                       Print warnings instead of failing.
--fail-on <level>                 `none`, `high-confidence-breaking`, `any-breaking`, `any-mismatch`.
--allow-missing-for <package>     Escape hatch for special cases.
```

MVP validation rules:

- If a package has high-confidence breaking impact, at least one pending changeset for that package must be `major`.
- If a package has high-confidence additive public API impact and no breaking impact, a pending changeset should be at least `minor`; this can be a warning in the MVP.
- If no public API impact is detected, do not force patch; leave patch/no-changeset policy to existing changeset/affected-package logic.

### 5. MCP/tooling API

Expose the same primary workflow to agents through MCP:

```text
monochange_classify_change
monochange_api_diff
monochange_validate_changeset_api_impact
```

`monochange_classify_change` should be the preferred agent tool because it avoids requiring the agent to compose low-level shell commands.

## Snapshot data model

The current `PackageSnapshot` is a collection of text files. For durable API snapshot files, introduce a normalized API snapshot model alongside it.

Proposed core types:

```rust
pub struct ApiSnapshot {
	pub schema_version: u32,
	pub package_id: String,
	pub package_name: String,
	pub ecosystem: Ecosystem,
	pub manifest_path: PathBuf,
	pub source_ref: Option<String>,
	pub analyzer_id: String,
	pub analyzer_version: String,
	pub items: Vec<ApiItem>,
	pub metadata: BTreeMap<String, serde_json::Value>,
	pub warnings: Vec<ApiSnapshotWarning>,
}

pub struct ApiItem {
	pub id: String,
	pub kind: String,
	pub path: String,
	pub signature: Option<String>,
	pub visibility: ApiVisibility,
	pub source_file: PathBuf,
	pub stability: ApiStability,
	pub attributes: BTreeMap<String, serde_json::Value>,
}
```

Important design detail: `id` should be stable and ecosystem-specific, while `path` should be human-readable.

Example Cargo item ids:

```text
cargo:function:monochange_core::version_plan::from_workspace
cargo:struct:monochange_core::VersionPlan
cargo:trait:monochange_core::ReleasePlanner
cargo:impl-method:monochange_core::VersionPlan::packages
```

Example npm item ids:

```text
npm:export:.
npm:export:./cli
npm:symbol:createChangeset
npm:symbol:ChangeClassification
npm:bin:mc
```

Example snapshot JSON:

```json
{
	"schemaVersion": 1,
	"packageId": "monochange_core",
	"packageName": "monochange_core",
	"ecosystem": "cargo",
	"manifestPath": "crates/monochange_core/Cargo.toml",
	"sourceRef": "working-tree",
	"analyzerId": "cargo/public-api-snapshot",
	"analyzerVersion": "1",
	"items": [
		{
			"id": "cargo:function:monochange_core::PackageName::new",
			"kind": "function",
			"path": "monochange_core::PackageName::new",
			"signature": "pub fn new(value: impl Into<String>) -> Self",
			"visibility": "public",
			"sourceFile": "src/lib.rs",
			"stability": "stable",
			"attributes": {}
		}
	],
	"metadata": {},
	"warnings": []
}
```

## Diff and impact data model

Raw diffs should be separate from bump recommendations.

```rust
pub struct ApiDiff {
	pub package_id: String,
	pub ecosystem: Ecosystem,
	pub analyzer_id: String,
	pub changes: Vec<ApiChange>,
	pub warnings: Vec<String>,
}

pub struct ApiChange {
	pub severity: ApiChangeSeverity,
	pub kind: ApiChangeKind,
	pub item_id: String,
	pub item_kind: String,
	pub item_path: String,
	pub before_signature: Option<String>,
	pub after_signature: Option<String>,
	pub source_file: Option<PathBuf>,
	pub confidence: ChangeConfidence,
	pub suggested_bump: SuggestedBump,
	pub summary: String,
}
```

Suggested enums:

```rust
pub enum ApiChangeSeverity {
	Breaking,
	Additive,
	Compatible,
	Unknown,
}

pub enum SuggestedBump {
	Major,
	Minor,
	Patch,
	None,
	Unknown,
}

pub enum ChangeConfidence {
	High,
	Medium,
	Low,
}
```

Rationale:

- `SemanticChangeKind::Modified` is not enough. A modified signature can be breaking, additive, compatible, or unknown depending on the ecosystem and item kind.
- Bump recommendation should be derived from impact, not baked into low-level symbol diffing.
- Confidence lets CI enforce high-confidence breaking changes while warning on weaker findings.

## Cargo MVP rules

Use the existing Rust parser in `monochange_cargo`, not `cargo-semver-checks`.

### Snapshot extraction

Cargo snapshot should include, at minimum:

- `pub` free functions
- `pub` structs
- `pub` enums
- `pub` traits
- `pub` type aliases
- `pub` constants/statics
- public modules
- public `pub use` re-exports
- public methods in inherent impl blocks when the type is public
- public trait methods when the trait is public

The current analyzer already extracts many of these. The implementation plan should first expose its internal `PublicSymbol` concept as normalized `ApiItem`s.

### Cargo impact classification rules

High-confidence major:

- Removed public item.
- Removed public re-export.
- Changed public function signature.
- Changed public method signature.
- Removed public trait method.
- Changed trait method signature.
- Removed public enum variant, if variants are captured.
- Removed public struct field from a public-field struct, if fields are captured.

High-confidence minor:

- Added public free function.
- Added public type, trait, module, const, or static.
- Added public inherent method.
- Added enum variant to a non-`#[non_exhaustive]` public enum is usually minor for consumers but can be source-breaking for exhaustive matches depending on enum exhaustiveness semantics. This should start as medium-confidence additive/unknown unless we model `#[non_exhaustive]`.

Medium-confidence patch/unknown:

- No public item diff but Rust source changed.
- Internal item changed.
- Analyzer warning prevented complete extraction.

Important limitations for MVP:

- `syn` source parsing does not resolve module trees as fully as rustc.
- `pub(crate)` and cfg-gated APIs require careful handling.
- Feature-gated APIs are difficult without feature selection.
- Type aliases and re-exports can hide breaking changes if not resolved.
- Generic bound changes may be breaking even if the rendered signature diff looks small.

Recommendation: be honest in the output. If the analyzer cannot be complete, emit a warning and lower confidence instead of pretending certainty.

## npm MVP rules

Use the existing npm/ECMAScript analyzer and package manifest analysis.

### Snapshot extraction

npm snapshot should include, at minimum:

- exported symbols from source files (`export`, `export default`, named re-exports)
- package `exports` map entries
- package `main`, `module`, `types`, and `bin` entries
- package `dependencies`, `peerDependencies`, and possibly `optionalDependencies`
- package type/module format metadata (`type`, source extension behavior)

The current `monochange_ecmascript` parser already extracts exported symbols/signatures. The current `monochange_npm` analyzer already compares manifest export/dependency/metadata changes.

### npm impact classification rules

High-confidence major:

- Removed exported symbol.
- Changed exported symbol signature.
- Removed package `exports` entry.
- Removed `main`, `module`, or `types` entry.
- Removed CLI binary entry from `bin`.
- Narrowed peer dependency range in a way likely to reject previously valid consumers.
- Removed dependency that is part of a runtime/public contract, when identifiable.

High-confidence minor:

- Added exported symbol.
- Added package `exports` entry.
- Added optional public entrypoint.
- Added CLI binary entry.

Medium-confidence patch/unknown:

- Changed implementation with no exported symbol or manifest surface change.
- Changed dependency versions. This may be patch/minor/major depending on whether consumers observe it.
- Changed package metadata that does not affect runtime imports.

Important npm nuance:

- TypeScript type compatibility is not the same as textual signature equality.
- A source export signature can change textually without being breaking, or appear unchanged while a referenced type changed elsewhere.
- Package `exports` is a stronger public contract than arbitrary source files. For packages with an `exports` map, classify exported entrypoints from that map as the primary public surface.
- For packages without `exports`, fallback to common conventions (`main`, `types`, `src/index.ts`, etc.) with medium confidence.

## Dependency propagation policy

Do not hardcode one propagation behavior. Make it configurable.

Proposed config:

```toml
[change_detection]
dependency_breaking_policy = "public"

# Possible values:
# "none"   - do not propagate dependency breaking changes.
# "direct" - direct dependency major implies dependent major.
# "public" - propagate only when dependency is exposed in the dependent's public API.
# "all"    - any transitive breaking dependency can propagate.
```

Recommended default for libraries:

```toml
dependency_breaking_policy = "public"
```

Rationale:

- If crate/package `A` depends on `B` internally and `B` breaks, `A` may not need a major bump.
- If `A` exposes `B` types in public API, re-exports `B`, or exposes an npm export that is just a pass-through to `B`, then `A` probably does need a major bump.
- For a monorepo that publishes every package independently, users may still want `direct` or `all`; the choice should be explicit.

MVP recommendation:

- Implement `none` first for classification clarity.
- Add `direct` using the existing package graph.
- Add `public` once snapshots include enough type/export reference metadata.

## Configuration proposal

Add a future `[change_detection]` section to `monochange.toml`.

```toml
[change_detection]
enabled = true
default_base = "origin/main"
default_level = "signature"
dependency_breaking_policy = "none"

[change_detection.enforcement]
mode = "advisory"
fail_on = "high-confidence-breaking"

[change_detection.ecosystems.cargo]
enabled = true
features = "default"
include_tests = false

[change_detection.ecosystems.npm]
enabled = true
public_surface = "exports-first"
include_private_packages = false
```

Possible enforcement modes:

```text
advisory  - report only
warn      - warnings in validation
strict    - fail validation on configured mismatches
```

## Agent-facing skill update

Once implemented, update the monochange skill with a short mandatory step:

```md
## Changeset impact check

Before writing or finalizing `.changeset/*.md`, run:

`mc change classify --base origin/main --format markdown --for-agent`

Use the highest suggested bump per package unless you have explicit maintainer guidance. If the report shows high-confidence breaking API changes and you think a major bump is wrong, stop and ask the maintainer instead of silently downgrading to patch/minor.
```

The `--for-agent` output should be intentionally compact:

```md
# Agent change classification

- `monochange_core`: major required, high confidence
  - removed public function `PackageName::new`
- `monochange_npm`: minor suggested, high confidence
  - added export `classifyChange`

Required action:

- Create a major changeset for `monochange_core`.
- Create a minor changeset for `monochange_npm` unless this was unintentional.
```

## Implementation phases

### Phase 0: tighten language and product contract

- [x] Confirm command names: `mc change classify` vs `mc classify` vs extending `mc analyze`.
- [x] Confirm whether `mc changeset validate --api` should be new or folded into existing `mc validate`/`mc check` flows.
- [x] Confirm default base ref behavior (`origin/main`, with `--head HEAD`).
- [x] Confirm initial dependency propagation policy (`none` recommended for MVP).
- [x] Confirm whether snapshot files are purely ephemeral initially or can be written to `.monochange/api-snapshots/`.

### Phase 1: shared API snapshot model

- [x] Add normalized `ApiSnapshot`, `ApiItem`, `ApiDiff`, `ApiChange`, severity, confidence, and suggested bump types to `monochange_core`.
- [x] Add serialization tests for stable JSON output.
- [x] Add snapshot schema versioning.
- [x] Add helper methods for stable item sorting and deterministic output.
- [x] Preserve existing `SemanticChange` for compatibility; do not remove it in the MVP.

### Phase 2: Cargo snapshot adapter

- [x] Refactor `monochange_cargo::analysis` so public Rust symbol extraction can produce `ApiItem`s.
- [x] Keep existing `SemanticAnalyzer` output working.
- [x] Add tests for removed function, changed function signature, added function, removed struct, changed trait method.
- [x] Capture analyzer limitations as warnings.
- [x] Ensure extraction is fast and does not call rustdoc/rustc.

### Phase 3: npm snapshot adapter

- [x] Refactor `monochange_ecmascript` exported symbols into `ApiItem`s.
- [x] Add npm manifest public-surface items for `exports`, `main`, `types`, `bin`, and key dependency categories.
- [x] Keep existing `NpmSemanticAnalyzer` output working.
- [x] Add tests for removed export, changed exported signature, added export, removed package export entry, changed `bin` entry.
- [x] Treat package `exports` as primary surface when present.

### Phase 4: API diff and impact classifier

- [x] Implement generic snapshot diffing keyed by `ApiItem.id`.
- [x] Implement ecosystem-specific classification hooks for Cargo and npm.
- [x] Map existing semantic changes into severity/confidence/suggested bump for the first usable MVP.
- [x] Add JSON and markdown renderers.
- [x] Add tests for bump aggregation:
  - any high-confidence breaking → package major
  - additive only → package minor
  - no public surface changes → patch/unknown/no recommendation
  - analyzer warnings → lower confidence

### Phase 5: CLI commands

- [x] Add `mc api snapshot`.
- [x] Add `mc api diff`.
- [x] Add `mc change classify` or selected equivalent.
- [x] Add `--format json` and markdown output coverage.
- [x] Add integration tests in `crates/monochange_integration_tests` with fixtures and Insta snapshots.

### Phase 6: changeset validation

- [x] Add `mc changeset validate --api` or selected equivalent.
- [x] Compare pending changeset bumps against classification results.
- [x] Start with advisory/warn-only behavior unless strict flag is passed.
- [x] Add override mechanism for false positives.
- [x] Add tests for major-required, minor-suggested, no-public-impact, and override flows.

### Phase 7: MCP and skill integration

- [x] Add MCP tool for change classification.
- [x] Add MCP tool or validation option for changeset/API mismatch.
- [x] Update monochange skill to require the classification step before changeset authoring.
- [x] Add examples showing agent workflow.

### Phase 8: future analyzers

Future analyzers should plug into the same normalized snapshot model.

Candidates:

- CLI command tree snapshots.
- JSON schema/config snapshots.
- Dart package API snapshots.
- Python package API snapshots.
- Java/Kotlin package API snapshots.
- Swift package API snapshots.

## Future CLI surface analysis

CLI changes are public API changes too, but they should be a later analyzer.

Treat CLI as a module tree:

```text
mc
├── change
│   └── classify
├── api
│   ├── snapshot
│   └── diff
└── changeset
    └── validate
```

CLI `ApiItem`s could include:

- commands
- subcommands
- flags
- positional arguments
- accepted enum values
- defaults
- environment variables
- output formats

Breaking CLI changes:

- removed command
- removed flag
- removed positional argument support
- made optional arg required
- removed accepted enum value
- changed JSON output schema incompatibly

Minor CLI changes:

- added command
- added optional flag
- added accepted enum value
- added optional JSON output field

Best implementation path:

- Define generic CLI snapshot JSON.
- For Rust/clap CLIs, export the command tree directly from clap rather than scraping help text.
- For arbitrary CLIs, fallback to help scraping with lower confidence.

## Future Dart/Python analyzer notes

Dart and Python are harder because monochange may not have Rust-native parser support equivalent to `syn`/`oxc`.

Possible approaches:

1. Lightweight parser written in Rust for just public surface extraction.
2. Tree-sitter-based parser, if dependency size and maintenance are acceptable.
3. External tool adapters, if the ecosystem has a fast stable API dumper.
4. Language-server/protocol based extraction, probably too heavy for the default agent loop.

Recommendation:

- Do not block the MVP on Dart/Python.
- Design `ApiSnapshot` so these analyzers can emit the same `ApiItem` shape later.
- Prefer "good enough public-surface extraction" over complete semantic type checking for agent guidance.

## Override design

False positives must be easy to handle explicitly.

Possible changeset comment override:

```md
<!-- monochange-api-impact: allow major-to-minor because removed item was never reachable from documented exports -->
```

Possible TOML override:

```toml
[change_detection.overrides]
"monochange_core:cargo:function:internal::legacy" = "ignore"
```

Possible CLI flag:

```nushell
mc changeset validate --api --allow-impact-override
```

Recommendation:

- Prefer changeset-local overrides because they keep the reasoning next to release intent.
- Require a human-readable reason.
- Surface overrides in reports so they do not become invisible technical debt.

## Suggestions and product feedback

### Suggestion 1: make `mc change classify` the centre, not `mc api diff`

`mc api diff` is useful for developers, but agents should have one obvious command. The skill should tell agents to run `mc change classify`, not assemble lower-level commands.

### Suggestion 2: separate detection from recommendation

Keep these layers distinct:

1. Snapshot extraction: what public surface exists?
2. Diffing: what changed?
3. Impact classification: is it breaking/additive/compatible/unknown?
4. Release recommendation: major/minor/patch/none?
5. Changeset validation: does release intent match the evidence?

This keeps the system extensible and avoids baking release policy into every analyzer.

### Suggestion 3: default to advisory, enforce later

The first implementation should build trust. Make reports easy to inspect and only later turn on strict validation for high-confidence breaking changes.

### Suggestion 4: use confidence aggressively

For fast parsers, incomplete knowledge is normal. Confidence gives us a safe way to be useful without pretending the analyzer is a full compiler.

### Suggestion 5: make docs/no-public-impact explicit

A report that says "no public API impact detected" is valuable. It gives the agent evidence that a patch/no-changeset decision may be reasonable, while still leaving documentation policy to the repo.

### Suggestion 6: benchmark the command as part of acceptance

Because speed is a product requirement, include a performance budget.

Suggested MVP target for this repo:

```text
mc change classify --base origin/main --format json
```

should complete in a few seconds on typical small/medium PRs, because it only snapshots affected packages and avoids rustdoc/rustc.

## Acceptance criteria for the MVP

- `mc change classify --base <ref> --format json` returns package-level suggested bumps for Cargo and npm packages.
- `mc change classify --base <ref> --format markdown --for-agent` is concise enough to put directly into agent instructions.
- Cargo removed/modified public functions produce high-confidence major recommendations.
- Cargo added public functions produce high-confidence minor recommendations.
- npm removed/modified exports produce high-confidence major recommendations.
- npm added exports produce high-confidence minor recommendations.
- Manifest export removals in npm produce high-confidence major recommendations.
- Analyzer warnings are surfaced and lower confidence where appropriate.
- Existing `mc analyze` behavior remains compatible or has a documented migration path.
- Changeset validation can warn when a high-confidence major recommendation is paired with a patch/minor changeset.

## Open questions

1. Should the primary command be `mc change classify`, `mc changeset suggest`, or an extension of `mc analyze`?
2. Should snapshot files ever be committed, or should they remain ephemeral/generated by default?
3. Should `dependency_breaking_policy` default to `none` for MVP or `public` for ideal semantics?
4. How strict should npm dependency range changes be classified in the first version?
5. Should `docs-only` changes map to `none`, `patch`, or stay outside this classifier?
6. Should CLI/API analyzers be configured per package or as top-level workspace analyzers?

## Proposed next step

Create an implementation issue/plan for Phase 1 through Phase 5 only:

1. Shared API snapshot model.
2. Cargo adapter.
3. npm adapter.
4. Diff + classification.
5. `mc change classify` command.

Defer strict changeset validation and future ecosystem analyzers until the classification output is proven useful in real agent workflows.
