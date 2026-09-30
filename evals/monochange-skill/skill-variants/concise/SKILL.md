---
name: monochange
description: Configure monochange monorepo release planning, maintain changesets, inspect versions and release notes, and operate configured CLI or MCP workflows. Use for monochange.toml, .changeset files, multi-ecosystem release plans, or the monochange MCP server.
---

# monochange

monochange discovers Cargo, npm, Deno, Dart/Flutter, Python, and Go packages and combines configuration with changesets to plan releases. Use this binary's `--help` and bundled topics for current contracts. Read focused references only for the task at hand.

## Start from the repository

Read the repository instructions and `monochange.toml`, if present. Identify configured package and group ids, version ownership, changelog streams, and `[cli.*]` workflows. `monochange discover --format json`, `monochange config`, and `monochange help` expose that model. `monochange step validate` can succeed without a config; check its presence separately.

Use `monochange run <name>` only for a workflow the config defines, after inspecting its steps. Short built-ins such as `create`, `discover`, `preview`, `prepare`, `affected`, and `diagnose` work independently of workflow definitions. Every typed built-in step also has `monochange step <step-name>`; `Command` steps only belong in configured workflows.

Keep actions within the user's existing authorization and repository rules. Perform requested local edits and preparation, review the resulting diff, and preserve separate authorization boundaries for commits, provider changes, tags, and publication.

## Choose the relevant reference

- **Initialize or update adoption:** read [skills/adoption.md](skills/adoption.md). `init` generates a starter; the legacy `populate` command currently adds nothing because the default workflow set is empty. Edit `[cli.*]` or use `command` to customize workflows. Refresh an installed skill from the upgraded binary with `skill install --dir <directory> --force`, after reviewing local customizations.
- **Configure packages, groups, prereleases, versioned files, or workflows:** read [skills/configuration.md](skills/configuration.md). Native version writers cover Cargo, npm, Deno, and Dart. Python's own version needs an explicit typed `versioned_files` entry with `fields = ["version"]`; Go versions come from tags.
- **Create or revise release intent:** read [skills/changesets.md](skills/changesets.md). Inspect pending files first, target configured ids, and keep every file within one changelog stream. Write separate entries for developer and user audiences when both matter.
- **Decide severity or validate API intent:** read [skills/change-classification.md](skills/change-classification.md). `monochange change classify --detection-level semantic --format json --dependency-propagation public` supplies evidence; partial analyzer coverage still requires review. Respect configured type mappings and pre-1.0 bump rules.
- **Select command flags or parse output:** read [skills/commands.md](skills/commands.md). Use `--format json` without `--quiet` for machine-readable stdout. Inspect actual fields before extracting data.
- **Apply manifest policy:** read [skills/linting.md](skills/linting.md).
- **Inspect readiness or prepare publication automation:** read [skills/multi-package-publishing.md](skills/multi-package-publishing.md) and, for registry trust, [skills/trusted-publishing.md](skills/trusted-publishing.md). These references explain the tool's capabilities; project restrictions and authorization still govern actions.
- **Need more detail or examples:** use [skills/readme.md](skills/readme.md), [skills/reference.md](skills/reference.md), or [examples/readme.md](examples/readme.md).

## Verify the outcome

Run `monochange step validate` for parsing and target resolution, `monochange check` for configured manifest linting, and `monochange preview --format json` for the complete release plan. Preview catches cross-stream changesets that the first two checks do not. Inspect package/group versions, changed files, changelog streams and outputs, and relevant warnings.

`monochange preview` forces dry-run planning. `monochange prepare` writes release files and consumes applied changesets. `monochange versions list` inventories current versions; `monochange versions sync --dry-run` previews dependency synchronization without computing release bumps. `monochange notes --output <id> [--target <id>]` renders a named artifact without consuming changesets; `--file` redirects it to that file.

Only `.monochange/local/` is local scratch space. Keep release records and prerelease state under `.monochange/` committed; ignoring the parent directory makes later release operations lose their source of truth.

If the repository exposes MCP, use its structured tools for discovery, classification, changeset validation, and release preview. Use the CLI when reproducing the exact workflow maintainers run. Report the observable result, verification performed, and remaining limits.
