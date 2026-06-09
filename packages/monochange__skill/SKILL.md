---
name: monochange
description: Use the monochange CLI and MCP tooling to configure monorepo package versioning, create changesets, preview releases, generate versioned package files, and run configured publish workflows. Use when working with monochange.toml, .changeset/*.md, [cli.*] workflow commands, package/group version plans, manifest linting, release records, or the monochange MCP server.
---

# monochange

Use this skill when the user wants to operate monochange in a repository or author a `monochange.toml` configuration for versioned package releases.

monochange is a release-planning harness rather than a single fixed workflow. It discovers package manifests, maps them to configured package and group ids, reads `.changeset/*.md` release intent, computes versions, updates native manifests and extra versioned files, and then exposes release, source-provider, and package-publishing actions through built-in steps or repository-defined workflows.

Agents should optimize for safety and traceability: inspect config first, prefer JSON/dry-run output while planning, preserve changeset intent in files, and only run mutating release or publish flows after the user has approved the exact command path.

## Source-of-truth rules

- Read `monochange.toml` before recommending commands. Top-level `monochange <name>` workflow commands can be user-defined by `[cli.<name>]` and vary per repository.
- Do not assume `discover`, `change`, `release`, `publish`, or similar configured workflow names exist in every repo. They are user-defined and should be invoked as `monochange run <name>` only when they appear in `monochange help` for that workspace.
- Binary commands are wired by the CLI. Step commands are always exposed as `monochange step <step-name>` for built-in step variants, except the generic `Command` step.
- When authoring `[cli.*]` workflows, command inputs are explicit per step. Add `inputs = ["name"]` on a step to inherit a command input unchanged, or use the map form for overrides and renamed values.
- Prefer package or group ids from `monochange.toml` over manifest names.
- Use dry-run or preview commands before mutating versions, committing, tagging, releasing, or publishing.
- Never publish with local credentials on behalf of a user unless they explicitly own that operation and the project rules allow it.

## Fast workflow

1. Inspect configuration: `monochange step validate`, `monochange step config`, or `monochange help`. Use this to learn package ids, enabled ecosystems, groups, and which top-level workflow commands actually exist.
2. Inspect packages: use the configured workflow command (often `monochange step discover --format json`) or `monochange step discover --format json`. Prefer JSON when another tool or agent will consume the package graph.
3. Classify API impact before writing release intent: run `monochange change classify --base origin/main --format markdown --dependency-propagation public` (or use the `monochange_classify_changes` MCP tool) and use the recommended bumps as the starting point for changesets. Omit `--dependency-propagation public` when you only want packages with direct source changes.
4. Create release intent: use a configured workflow command (often `monochange run change ...`) or write `.changeset/*.md` manually. Read existing changesets first so you can update or merge related intent instead of creating duplicates.
5. Preview versioned files: use the configured workflow command (often `monochange run release --dry-run --format json` or `--diff`) or `monochange step prepare-release --dry-run`. The preview is where you verify versions, changelog entries, generated manifests, lockfile work, and semantic SemVer `compatibilityEvidence` before mutating the tree.
6. Run validation and linting: `monochange check`, `monochange step validate`, and `monochange changeset validate --api --base origin/main`. `validate` catches monochange configuration and target issues; `check` also runs manifest lint rules.
7. Only after review, run configured commit/release/publish workflows. Keep release-record, readiness, bootstrap, plan, and publish artifacts when the workflow emits them.

## What to open next

- [skills/readme.md](skills/readme.md) — index of all focused skill modules.
- [skills/commands.md](skills/commands.md) — verified built-in commands, step commands, user-defined command behavior, and all CLI step types.
- [skills/configuration.md](skills/configuration.md) — current `monochange.toml` structure and examples.
- [skills/changesets.md](skills/changesets.md) — changeset file shape, CLI creation, and lifecycle rules.
- [skills/linting.md](skills/linting.md) — `monochange check`, lint presets, and manifest policy.
- [skills/multi-package-publishing.md](skills/multi-package-publishing.md) — readiness, bootstrap, and package publishing flows.
- [skills/trusted-publishing.md](skills/trusted-publishing.md) — registry trust/OIDC notes for publishing.
- [skills/reference.md](skills/reference.md) — full operating guide.
- [examples/readme.md](examples/readme.md) — copyable example scenarios.

## Verified command inventory

The command inventory in this skill is based on `crates/monochange_cli/src/cli.rs`, `crates/monochange_core/src/lib.rs`, and the CLI help snapshot `crates/monochange_cli/tests/snapshots/cli_help__help_overview_lists_all_commands@help_overview_lists_all_commands.snap`.

Built-in commands in the current CLI:

- `monochange init` — create a starter `monochange.toml` from discovered manifests.
- `monochange populate` — add missing configurable workflow definitions to an existing config.
- `monochange skill` — install or update the monochange skill bundle.
- `monochange subagents` — generate repository-local agent/subagent guidance for monochange work.
- `monochange analyze` — inspect semantic changes for a package.
- `monochange change classify --base origin/main --format markdown --dependency-propagation public` — classify API-impacting semantic changes, including direct public dependents, and recommend package bumps.
- `monochange api diff --base origin/main --format json` — inspect API diff classification as structured data.
- `monochange api snapshot --head HEAD --format json` — inspect the current monochange API snapshot report shape.
- `monochange changeset validate --api --base origin/main --format markdown` — advisory validation comparing API classification expectations with changeset intent; use this in CI as a non-blocking comment/check first.
- `monochange step tag-release` — create release tags from an embedded release record.
- `monochange step release-record` — inspect the release record reachable from a tag or commit.
- `monochange check` — validate configuration, changesets, and manifest lint rules.
- `monochange lint` — list or explain lint rules and presets.
- `monochange mcp` — run the stdio MCP server.
- `monochange step validate` — validate `monochange.toml` and changeset targets.
- `monochange step publish-readiness` — verify publishability from a release record without publishing.
- `monochange step placeholder-publish` — publish first-time placeholder versions for packages in a release record.
- `monochange versions` — synchronize internal workspace dependency constraints across all supported ecosystems.

Built-in step commands:

- `monochange step config`
- `monochange step validate`
- `monochange step discover`
- `monochange step display-versions`
- `monochange step create-change-file`
- `monochange step prepare-release`
- `monochange step commit-release`
- `monochange step verify-release-branch`
- `monochange step publish-release`
- `monochange step placeholder-publish`
- `monochange step publish-packages`
- `monochange step plan-publish-rate-limits`
- `monochange step open-release-request`
- `monochange step comment-released-issues`
- `monochange step affected-packages`
- `monochange step diagnose-changesets`
- `monochange step retarget-release`

`Command` is a valid `[cli.*].steps[].type` for running shell commands, but it is not exposed as `monochange step command`.

## Current MCP tools

The `monochange mcp` server exposes these tools:

- `monochange_validate` — validate config and changeset targets.
- `monochange_discover` — return structured packages, groups, dependencies, and ecosystems.
- `monochange_diagnostics` — inspect pending changesets with git and review context.
- `monochange_change` — create a changeset through structured tool input.
- `monochange_release_preview` — run a dry-run release preview.
- `monochange_release_manifest` — produce a release manifest payload for downstream automation.
- `monochange_affected_packages` — evaluate changed paths and changeset coverage.
- `monochange_lint_catalog` — list lint rules and presets.
- `monochange_lint_explain` — explain one lint rule or preset.
- `monochange_analyze_changes` — inspect semantic diffs for package-aware changes.
- `monochange_classify_changes` — classify API-impacting changes and return package bump recommendations.
- `monochange_validate_changeset` — check one changeset against the current semantic diff.

Prefer MCP tools when the caller needs structured data and the shell when you need to run the exact repository workflow that maintainers use locally or in CI.

## Semantic SemVer guardrails

Release planning treats semantic analysis as advisory guardrails. When a git change frame can be analyzed, `monochange run release --dry-run --format json`, `monochange_release_preview`, and release manifests may include `compatibilityEvidence` inferred from public API/export, dependency, and metadata changes. Compare this with human-authored changesets:

- removed or modified public API/export evidence implies at least `major`;
- added public API/export evidence implies at least `minor`;
- dependency or metadata evidence is usually `patch` context;
- warnings about semantic changes without matching changesets should be resolved before release.

Do not let semantic analysis author releases automatically. Use it to catch mismatched or missing changesets, then update `.changeset/*.md` deliberately.

For comparing two refs today, use `monochange analyze`:

```nu
monochange analyze --package core --main-ref <base-ref> --head-ref <head-ref>
```

For release-aware trajectory:

```nu
monochange analyze --package core --release-ref <last-release-tag> --main-ref main --head-ref HEAD --format json
```
