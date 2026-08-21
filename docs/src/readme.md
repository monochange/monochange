# Introduction

`monochange` is a cross-ecosystem release planner for monorepos.

It is easiest to learn with one safe local walkthrough before you touch provider publishing, release PRs, diagnostics, or MCP setup.

## Who this guide is for

- maintainers of monorepos that span more than one package ecosystem
- teams replacing ad hoc release scripts with explicit change files
- people who want a predictable release plan before adding automation

## Start with one safe walkthrough

Install the prebuilt CLI from npm:

```bash
npm install -g @monochange/cli
monochange --help
```

Then run the core beginner flow:

<!-- {=projectCoreWorkflow} -->

Generate a starter config from the packages monochange detects:

```bash
monochange init
```

`monochange init` writes an annotated, minimal `monochange.toml` without default `[cli.*]` workflow aliases. The binary exposes immutable `monochange step *` commands for every built-in step when you need a direct, config-free entry point; add `[cli.*]` tables only for repository-specific named workflows.

For automated CI setup, include the `--provider` flag:

```bash
monochange init --provider github
```

This configures the `[source]` section, generates CLI commands for `commit-release` and `release-pr`, and creates GitHub Actions workflows.

Validate the workspace:

```bash
monochange step validate
```

Discover the package ids you will use in commands and changesets:

```bash
monochange step discover --format json
```

Create one change file for a package id:

```bash
monochange run change --package <id> --bump patch --reason "describe the change"
```

Most changes should target a package id. Use group ids only when the change is intentionally owned by the whole group.

When a package is only changing because another dependency or version group moved first, author that context explicitly instead of relying on anonymous propagation:

```bash
monochange run change --package <dependent-id> --bump none --caused-by <upstream-id> --reason "dependency-only follow-up"
```

Preview the release plan safely:

```bash
monochange run release --dry-run --format json
```

Add `--diff` when you want unified file previews for version and changelog updates without mutating the workspace:

```bash
monochange run release --dry-run --diff
```

This first run is safe: nothing is published. Stop here until you are ready to prepare release files locally.

When you are ready to prepare the release locally, run `monochange run release`.

<!-- {/projectCoreWorkflow} -->

For human-readable local output, `monochange run release --dry-run` defaults to terminal-friendly markdown. Use `--format json` for automation, `--format text` when you explicitly want the older plain-text rendering, and `--quiet` when you want dry-run behavior without stdout/stderr output. Use `monochange step display-versions` when you only need planned package and group versions; use `monochange versions --dry-run` when you want to preview internal dependency constraint updates before writing them.

This book is maintained with `mdt` so shared content blocks stay synchronized across pages. See the [Configuration reference](guide/04-configuration.md#shared-documentation) for how template updates work.

If you want a slower, more guided walkthrough, continue with [Start here](./guide/00-start-here.md) and [Your first release plan](./guide/02-setup.md).

## What to read next

- [Start here](./guide/00-start-here.md): install, `monochange init`, validation, discovery, and `--dry-run`
- [Installation](./guide/01-installation.md): npm, Cargo, optional assistant tooling, and repository development setup
- [Your first release plan](./guide/02-setup.md): generated config first, package ids before groups
- [Configuration reference](./guide/04-configuration.md): the full package, group, changelog, and CLI model
- [Release planning](./guide/06-release-planning.md): changesets, dry runs, diff previews, and planning rules
- [Advanced: GitHub automation](./guide/08-github-automation.md): provider publishing and release requests
- [Advanced: CI, package publishing, and release PR flows](./guide/13-ci-and-publishing.md): per-provider CI patterns, trusted publishing, and long-running release PR design notes
- [Advanced: Assistant setup and MCP](./guide/09-assistant-setup.md): optional AI-assisted workflows
- [Reference: Manifest linting with `monochange check`](./reference/linting.md): `[lints]` rules for Cargo and npm-family manifests

<!-- {=projectRecentPublishingImprovements} -->

### Recent package publishing improvements

Recent `monochange` improvements made package publishing guidance and diagnostics much more actionable:

- a dedicated trusted-publishing guide covers `npm`, `crates.io`, `jsr`, and `pub.dev`
- CI examples prefer the official registry-maintained workflows for `crates.io` and `pub.dev`
- a dedicated multi-package publishing guide covers monorepo tag, workflow, and package-boundary patterns
- CLI output gives clearer manual next steps for registries that still require registry-side trusted-publishing enrollment
- built-in publish preflight validates and reports the expected GitHub repository, workflow, and environment context for manual registries when it can infer them
- the monochange repository wires `monochange run publish-check` as a dry-run `PublishPackages` workflow so CI can verify package-publishing readiness without publishing

<!-- {/projectRecentPublishingImprovements} -->

## Command and automation matrix

<!-- {=projectCommandAutomationMatrix} -->

These are common commands for repositories using monochange. With the current CLI model, workflow names such as `discover`, `change`, `release`, `publish`, and `affected` come from optional `[cli.*]` tables in `monochange.toml` and run as `monochange run <name>`; binary commands such as `check`, `init`, `versions`, and `mcp` stay built in, while typed built-in operations such as validation are exposed as immutable `monochange step *` commands.

| Goal                             | Command                                                                                  | Use it when                                                                                                                        |
| -------------------------------- | ---------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Validate config and changesets   | `monochange step validate`                                                               | You changed `monochange.toml` or `.changeset/*.md` files                                                                           |
| Inspect package ids and groups   | `monochange step discover --format json`                                                 | You need the normalized workspace model                                                                                            |
| Sync internal dependency ranges  | `monochange versions --dry-run`                                                          | You want internal dependency references to match canonical workspace package versions                                              |
| Create release intent            | `monochange run change --package <id> --bump <severity> --reason "..."`                  | You need a new `.changeset/*.md` file                                                                                              |
| Audit pending release context    | `monochange step diagnose-changesets --format json`                                      | You need git provenance, PR/MR links, or related issues                                                                            |
| Preview the release plan         | `monochange run release --dry-run --diff` or `monochange step prepare-release --dry-run` | You want changelog/version patches without mutating the repo                                                                       |
| Create a durable release commit  | `monochange step commit-release`                                                         | You want a monochange-managed release commit with an embedded `ReleaseRecord`                                                      |
| Open or update a release request | `monochange step open-release-request`                                                   | You want a long-lived release PR/MR branch updated from current release state                                                      |
| Inspect a past release commit    | `monochange step release-record --from <ref>`                                            | You need the durable release declaration from git history                                                                          |
| Check package publish readiness  | `monochange step publish-readiness --from HEAD --output <path>`                          | You want a non-mutating preflight report before package publication                                                                |
| Dry-run configured publishing    | `monochange run publish-check`                                                           | This repository, or another repo with a similar `[cli.publish-check]`, should exercise publishing in CI without registry mutations |
| Plan ready package publishing    | `monochange step plan-publish-rate-limits --readiness <path>`                            | You want rate-limit batches that exclude non-ready package work                                                                    |
| Publish packages to registries   | `monochange step publish-packages --output <path>`                                       | You want `cargo publish`, `npm publish`, `deno publish`, or `dart pub publish` style package publication                           |
| Bootstrap release packages       | `monochange step placeholder-publish --from HEAD --output <path>`                        | You need a release-record-scoped placeholder bootstrap artifact before rerunning readiness                                         |
| Create post-merge release tags   | `monochange step tag-release --from HEAD`                                                | You merged a monochange release commit and now need to create and push its declared tag set                                        |
| Repair a recent release          | `monochange step retarget-release --from <tag> --target <commit>`                        | You need to retarget a just-created release to a later commit                                                                      |
| Publish hosted/provider releases | `monochange step publish-release`                                                        | You want GitHub/GitLab/Gitea release objects from prepared release state                                                           |

<!-- {/projectCommandAutomationMatrix} -->

`monochange step publish-readiness` performs non-mutating registry checks before `monochange step publish-packages`. For built-in Cargo publishes to crates.io it also verifies current manifest publishability: `publish = false` blocks publishing, `publish = [...]` must include `crates-io`, `description` must be set, and either `license` or `license-file` must be set. Workspace-inherited Cargo metadata is accepted, and already-published versions remain non-blocking in readiness reports. The artifact fingerprints `monochange.toml`, package manifests, lockfiles, and registry/tooling files, so rerun `monochange step publish-readiness` after those inputs change. `monochange step plan-publish-rate-limits --readiness <path>` validates the artifact for planning and limits rate-limit batches to package ids that are ready in both the artifact and the fresh local readiness check. `monochange step publish-packages` publishes directly from prepared release or `HEAD` release state and does not require the readiness artifact. If readiness shows missing first-time registry packages, run `monochange step placeholder-publish --from HEAD --output .monochange/bootstrap-result.json`, then rerun readiness before real publishing.

## What monochange can do

<!-- {=projectMilestoneCapabilities} -->

- discover Cargo, npm/pnpm/Bun, Deno, Dart, Flutter, Python, and Go packages
- normalize dependency edges across ecosystems
- coordinate shared package groups from `monochange.toml`
- compute release plans from explicit change input
- expose repository-defined workflow commands as `monochange run <command>` from `[cli.<command>]` definitions
- run config-defined release commands from `.changeset/*.md`
- render changelogs through structured release notes and configurable formats
- emit stable release-manifest JSON for downstream automation
- preview or publish provider releases and release requests from typed command steps and shared release data
- inspect durable release records from tags or descendant commits with `monochange step release-record`
- create post-merge release tags from a merged release commit with `monochange step tag-release --from HEAD`
- repair a recent source/provider release by retargeting its release tags with `monochange step retarget-release`
- inspect changeset context and review metadata with `monochange step diagnose-changesets` for both human and automation workflows
- apply Rust semver evidence when provided
- expose a bundled assistant skill plus a stdio MCP server with `monochange mcp`
- publish the CLI as `@monochange/cli` and the bundled agent skill as `@monochange/skill`
- publish end-user documentation through the mdBook in `docs/`

<!-- {/projectMilestoneCapabilities} -->

## What the JSON output includes

Discovery output includes:

<!-- {=projectDiscoveryOutputIncludes} -->

- normalized package records
- dependency edges
- release groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

Release-plan output includes:

<!-- {=projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items
- optional `fileDiffs` previews when you request `--diff`

<!-- {/projectReleaseOutputIncludes} -->

## Contributing to monochange itself

If you are working on the monochange repository, run the full local validation suite before opening a PR:

<!-- {=projectValidationCommands} -->

```bash
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->
