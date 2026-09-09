# Subagents and MCP

monochange ships two assistant-facing surfaces:

- `monochange subagents <target...>` generates repo-local agent, subagent, or rule files for supported harnesses
- `monochange mcp` starts a stdio MCP server so assistants can call monochange tools directly

## Classify changes before writing changesets

Run the JSON form when an agent needs to decide release intent:

```bash
monochange change classify --detection-level semantic --format json --dependency-propagation public
```

The report compares the pull request candidate with both the default branch and each package's latest release. It separates the current `proposedChangesetBump` from the accumulated `releaseFloor`, and every major or minor proposal links to specific findings. Read [Change classification](../reference/change-classification.md) for the full report contract and coverage limits.

After writing or updating the changesets, validate high-confidence evidence:

```bash
monochange changeset validate --api --format markdown
```

Package addition and removal findings use complete, high-confidence endpoint evidence. TypeScript packages can also produce complete, high-confidence declaration evidence in semantic mode when their compiler inputs are available. Other ecosystem source findings and TypeScript fallbacks remain advisory. Add `--strict` only when the repository wants every proposal to fail CI on a changeset mismatch.

## Install the CLI and skill

Install the CLI:

```bash
npm install -g @monochange/cli
monochange --help
```

Install the bundled skill into the current project:

```bash
monochange help skill
monochange skill
monochange skill --list
monochange skill -a pi -y
```

`monochange skill` forwards the remaining arguments to the upstream `skills add` workflow, so you can either keep its interactive prompts or pass the native `--agent`, `--skill`, `--copy`, `--all`, `--global`, and `--yes` flags directly.

<!-- {=assistantSkillBundleContents} -->

After copying the bundled skill, you get a small documentation set that is designed to load in layers:

- `SKILL.md`: concise entrypoint for agents
- `REFERENCE.md`: broader high-context reference with more examples
- `skills/README.md`: index of focused deep dives
- `skills/adoption.md`: setup-depth questions, migration guidance, and recommendation patterns
- `skills/change-classification.md`: release-aware severity decisions, uncertainty, and ecosystem review
- `skills/changesets.md`: changeset authoring and lifecycle guidance
- `skills/commands.md`: built-in command catalog and workflow selection
- `skills/configuration.md`: `monochange.toml` setup and editing guidance
- `skills/linting.md`: `[lints]` presets, `monochange check`, and manifest-focused examples
- `examples/README.md`: condensed scenario examples for quick recommendations

This layout keeps the top-level skill small while still making the richer guidance available when an assistant needs more context.

<!-- {/assistantSkillBundleContents} -->

## Generate repo-local subagents

Start with:

```bash
monochange help subagents
monochange subagents claude
monochange subagents pi codex
monochange subagents --all --dry-run --format json
```

Supported targets include:

- `claude`
- `vscode`
- `copilot`
- `pi`
- `codex`
- `cursor`

Generated subagents are CLI-first. They should prefer:

1. `monochange`
2. `npx -y @monochange/cli`

MCP config generation is optional and only emitted for targets with a stable repo-local MCP config format.

## MCP configuration

Typical client configuration:

<!-- {=mcpConfigSnippet} -->

```json
{
	"mcpServers": {
		"monochange": {
			"command": "monochange",
			"args": ["mcp"]
		}
	}
}
```

<!-- {/mcpConfigSnippet} -->

Start the server manually with:

```bash
monochange mcp
```

`monochange subagents` keeps MCP secondary. The generated files tell agents to prefer the CLI first and use MCP as an optional structured fallback.

## Recommended repo-local guidance

Keep instructions like these close to your project guidance:

<!-- {=assistantRepoGuidance} -->

- Read `monochange.toml` before proposing release workflow changes.
- Run `monochange step validate` before and after release-affecting edits.
- Use `monochange step discover --format json` to inspect package ids, group ownership, and dependency edges.
- Use `monochange step diagnose-changesets --format json` or `monochange_diagnostics` for a structured view of all pending changesets with git and review context.
- Run `monochange change classify --format json --dependency-propagation public` before writing release intent. Trace each proposed bump to its finding ids and review partial results.
- Use `monochange_lint_catalog` and `monochange_lint_explain` when you need lint metadata without shelling out.
- Prefer `monochange run change` plus `.changeset/*.md` files over ad hoc release notes.
- Use `monochange step prepare-release --dry-run --format json` before mutating release state.

<!-- {/assistantRepoGuidance} -->

## Current MCP tools

The MCP server is JSON-first and focuses on reviewable operations:

<!-- {=mcpToolsList} -->

- `monochange_validate`: validate `monochange.toml` and `.changeset` targets
- `monochange_discover`: discover packages, dependencies, and groups across the repository
- `monochange_diagnostics`: inspect pending changesets with git and review context as structured JSON
- `monochange_change`: write a `.changeset` markdown file for one or more package or group ids
- `monochange_release_preview`: prepare a dry-run release preview from discovered `.changeset` files
- `monochange_release_manifest`: generate a dry-run release manifest JSON document for downstream automation
- `monochange_affected_packages`: evaluate changeset policy from changed paths and optional labels
- `monochange_lint_catalog`: list registered manifest lint rules and presets
- `monochange_lint_explain`: explain one manifest lint rule or preset
- `monochange_analyze_changes`: analyze git diff state and return ecosystem-specific semantic changes
- `monochange_classify_changes`: compare the pull request and latest release, then return evidence-backed package bumps
- `monochange_validate_changeset`: validate one changeset against the current semantic diff

<!-- {/mcpToolsList} -->

These tools are designed to help assistants inspect the workspace, write explicit release intent, and preview release effects before a human or CI system performs mutating follow-up commands.

`monochange_analyze_changes` and `monochange_validate_changeset` provide semantic analysis across **Cargo, npm, Deno, and Dart/Flutter** packages. They surface ecosystem-specific evidence such as Rust public API diffs, JS/TS export changes, `package.json` and `deno.json` export metadata, and `pubspec.yaml` dependency or plugin-platform changes, then validate authored changesets against that semantic model.

When you need full changeset context, including the introduced commit, linked PR, and related issues, use `monochange step diagnose-changesets --format json` directly. It returns stable workspace-relative paths and structured records that agents can parse without reading raw markdown files.
