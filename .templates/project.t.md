<!-- {@projectReadmeOverview} -->

`monochange` is a release-planning toolkit for monorepos that span more than one package ecosystem.

It discovers packages, normalizes dependency data, applies group rules, turns explicit change files into release plans, and can run config-defined release preparation from those same inputs.

Use it when your repository has outgrown one-ecosystem release tooling and you want one model for Cargo, npm/pnpm/Bun, Deno, Dart/Flutter, Python, and Go.

<!-- {/projectReadmeOverview} -->

<!-- {@projectWhyUse} -->

- use one release-planning model across several language ecosystems
- replace ad hoc scripts with explicit change files and deterministic release output
- keep related packages synchronized with `[group.<id>]`
- propagate dependent bumps through one normalized dependency graph
- expose repository-defined workflow commands as `monochange run <command>` from `[cli.<command>]` entries in `monochange.toml`

<!-- {/projectWhyUse} -->

<!-- {@projectCrateCatalog} -->

| Crate                   | Badges                                                                                                                                                                                                                                                                               | Description                                                                                     |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------- |
| `monochange`            | [![Crates.io](https://img.shields.io/badge/crates.io-monochange-orange?logo=rust)](https://crates.io/crates/monochange) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange-1f425f?logo=docs.rs)](https://docs.rs/monochange/)                                               | end-user CLI and orchestration layer for discovery, planning, and CLI-defined release commands. |
| `monochange_core`       | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust)](https://crates.io/crates/monochange_core) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs)](https://docs.rs/monochange_core/)                         | shared domain model for packages, dependency edges, groups, change signals, and release plans.  |
| `monochange_config`     | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__config-orange?logo=rust)](https://crates.io/crates/monochange_config) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__config-1f425f?logo=docs.rs)](https://docs.rs/monochange_config/)                 | loads `monochange.toml`, parses `.changeset/*.md`, and validates CLI command inputs.            |
| `monochange_graph`      | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__graph-orange?logo=rust)](https://crates.io/crates/monochange_graph) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__graph-1f425f?logo=docs.rs)](https://docs.rs/monochange_graph/)                     | propagates release impact through dependency edges and synchronized groups.                     |
| `monochange_github`     | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__github-orange?logo=rust)](https://crates.io/crates/monochange_github) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__github-1f425f?logo=docs.rs)](https://docs.rs/monochange_github/)                 | converts release manifests into GitHub release payloads and publishing operations.              |
| `monochange_gitlab`     | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__gitlab-orange?logo=rust)](https://crates.io/crates/monochange_gitlab) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__gitlab-1f425f?logo=docs.rs)](https://docs.rs/monochange_gitlab/)                 | converts release manifests into GitLab release payloads and merge-request operations.           |
| `monochange_gitea`      | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__gitea-orange?logo=rust)](https://crates.io/crates/monochange_gitea) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__gitea-1f425f?logo=docs.rs)](https://docs.rs/monochange_gitea/)                     | converts release manifests into Gitea release payloads and pull-request operations.             |
| `monochange_forgejo`    | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__forgejo-orange?logo=rust)](https://crates.io/crates/monochange_forgejo) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__forgejo-1f425f?logo=docs.rs)](https://docs.rs/monochange_forgejo/)             | converts release manifests into Forgejo automation requests.                                    |
| `monochange_hosting`    | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__hosting-orange?logo=rust)](https://crates.io/crates/monochange_hosting) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__hosting-1f425f?logo=docs.rs)](https://docs.rs/monochange_hosting/)             | shared release-request abstractions for GitHub, GitLab, Gitea, and Forgejo providers.           |
| `monochange_publish`    | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__publish-orange?logo=rust)](https://crates.io/crates/monochange_publish) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__publish-1f425f?logo=docs.rs)](https://docs.rs/monochange_publish/)             | publishing support and trusted-publishing capability helpers for package registries.            |
| `monochange_ecmascript` | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__ecmascript-orange?logo=rust)](https://crates.io/crates/monochange_ecmascript) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__ecmascript-1f425f?logo=docs.rs)](https://docs.rs/monochange_ecmascript/) | shared JavaScript/TypeScript ecosystem utilities for npm, Deno, and JSR discovery.              |
| `monochange_semver`     | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__semver-orange?logo=rust)](https://crates.io/crates/monochange_semver) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__semver-1f425f?logo=docs.rs)](https://docs.rs/monochange_semver/)                 | merges requested bumps with compatibility-provider evidence.                                    |
| `monochange_telemetry`  | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__telemetry-orange?logo=rust)](https://crates.io/crates/monochange_telemetry) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__telemetry-1f425f?logo=docs.rs)](https://docs.rs/monochange_telemetry/)     | local-only telemetry event sink and privacy-preserving event schema helpers.                    |
| `monochange_cargo`      | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__cargo-orange?logo=rust)](https://crates.io/crates/monochange_cargo) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__cargo-1f425f?logo=docs.rs)](https://docs.rs/monochange_cargo/)                     | Cargo discovery plus Rust semver evidence integration.                                          |
| `monochange_npm`        | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__npm-orange?logo=rust)](https://crates.io/crates/monochange_npm) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__npm-1f425f?logo=docs.rs)](https://docs.rs/monochange_npm/)                             | npm, pnpm, and Bun workspace discovery.                                                         |
| `monochange_deno`       | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__deno-orange?logo=rust)](https://crates.io/crates/monochange_deno) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__deno-1f425f?logo=docs.rs)](https://docs.rs/monochange_deno/)                         | Deno workspace and package discovery.                                                           |
| `monochange_dart`       | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__dart-orange?logo=rust)](https://crates.io/crates/monochange_dart) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__dart-1f425f?logo=docs.rs)](https://docs.rs/monochange_dart/)                         | Dart and Flutter workspace discovery.                                                           |
| `monochange_python`     | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__python-orange?logo=rust)](https://crates.io/crates/monochange_python) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__python-1f425f?logo=docs.rs)](https://docs.rs/monochange_python/)                 | Python uv workspace, Poetry, and pyproject.toml discovery.                                      |
| `monochange_go`         | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__go-orange?logo=rust)](https://crates.io/crates/monochange_go) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__go-1f425f?logo=docs.rs)](https://docs.rs/monochange_go/)                                 | Go module discovery, go.mod dependency rewrites, and tag-based release metadata.                |
| `monochange_analysis`   | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__analysis-orange?logo=rust)](https://crates.io/crates/monochange_analysis) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__analysis-1f425f?logo=docs.rs)](https://docs.rs/monochange_analysis/)         | semantic diff analysis for Cargo, npm, Deno, and Dart/Flutter packages.                         |
| `monochange_changelog`  | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__changelog-orange?logo=rust)](https://crates.io/crates/monochange_changelog) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__changelog-1f425f?logo=docs.rs)](https://docs.rs/monochange_changelog/)     | changelog and release-note rendering.                                                           |
| `monochange_lint`       | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__lint-orange?logo=rust)](https://crates.io/crates/monochange_lint) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__lint-1f425f?logo=docs.rs)](https://docs.rs/monochange_lint/)                         | manifest lint rule definitions and presets.                                                     |
| `monochange_linting`    | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__linting-orange?logo=rust)](https://crates.io/crates/monochange_linting) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__linting-1f425f?logo=docs.rs)](https://docs.rs/monochange_linting/)             | the `monochange check` lint engine and reporters.                                               |
| `monochange_schema`     | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__schema-orange?logo=rust)](https://crates.io/crates/monochange_schema) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__schema-1f425f?logo=docs.rs)](https://docs.rs/monochange_schema/)                 | JSON Schema generation for `monochange.toml` editor support.                                    |
| `monochange_snapshot`   | [![Crates.io](https://img.shields.io/badge/crates.io-monochange__snapshot-orange?logo=rust)](https://crates.io/crates/monochange_snapshot) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange__snapshot-1f425f?logo=docs.rs)](https://docs.rs/monochange_snapshot/)         | deterministic snapshot-output helpers for CLI rendering.                                        |

<!-- {/projectCrateCatalog} -->

<!-- {@projectMilestoneCapabilities} -->

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

<!-- {@projectRecentPublishingImprovements} -->

### Publishing

Recent `monochange` improvements made package publishing guidance and diagnostics much more actionable:

- a dedicated trusted-publishing guide covers `npm`, `crates.io`, `jsr`, and `pub.dev`
- CI examples prefer the official registry-maintained workflows for `crates.io` and `pub.dev`
- a dedicated multi-package publishing guide covers monorepo tag, workflow, and package-boundary patterns
- CLI output gives clearer manual next steps for registries that still require registry-side trusted-publishing enrollment
- built-in publish preflight validates and reports the expected GitHub repository, workflow, and environment context for manual registries when it can infer them
- the monochange repository wires `monochange run publish-check` as a dry-run `PublishPackages` workflow so CI can verify package-publishing readiness without publishing

<!-- {/projectRecentPublishingImprovements} -->

<!-- {@projectCliAfterLongHelp} -->

### Quick Start

1. Create or update `monochange.toml` for your workspace:

```bash
monochange init
```

2. Validate configuration and changeset targets before making release changes:

```bash
monochange step validate
```

3. Inspect detected package ids and groups when authoring changesets or workflow inputs:

```bash
monochange step discover --format json
```

4. Use repository-defined workflows through `monochange run <command>` when they exist in your config, or call immutable built-in steps directly with `monochange step <name>`.

5. Preview before mutating files, publishing packages, creating tags, or opening release requests:

```bash
monochange run release --dry-run --diff
monochange step prepare-release --dry-run --diff
```

Run `monochange help <command>` or `monochange help step <name>` for command-specific options.

<!-- {/projectCliAfterLongHelp} -->

<!-- {@projectCommandAutomationMatrix} -->

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
| Bootstrap release packages       | `monochange step placeholder-publish`                                                    | You need a release-record-scoped placeholder bootstrap artifact before rerunning readiness                                         |
| Create post-merge release tags   | `monochange step tag-release --from HEAD`                                                | You merged a monochange release commit and now need to create and push its declared tag set                                        |
| Repair a recent release          | `monochange step retarget-release --from <tag> --target <commit>`                        | You need to retarget a just-created release to a later commit                                                                      |
| Publish hosted/provider releases | `monochange step publish-release`                                                        | You want GitHub/GitLab/Gitea release objects from prepared release state                                                           |

<!-- {/projectCommandAutomationMatrix} -->

`monochange step publish-readiness` performs non-mutating registry checks before `monochange step publish-packages`. For built-in Cargo publishes to crates.io it also verifies current manifest publishability: `publish = false` blocks publishing, `publish = [...]` must include `crates-io`, `description` must be set, and either `license` or `license-file` must be set. Workspace-inherited Cargo metadata is accepted, and already-published versions remain non-blocking in readiness reports. The artifact fingerprints `monochange.toml`, package manifests, lockfiles, and registry/tooling files, so rerun `monochange step publish-readiness` after those inputs change. `monochange step plan-publish-rate-limits --readiness <path>` validates the artifact for planning and limits rate-limit batches to package ids that are ready in both the artifact and the fresh local readiness check. `monochange step publish-packages` publishes directly from prepared release or `HEAD` release state and does not require the readiness artifact. If readiness shows missing first-time registry packages, run `monochange step placeholder-publish`, then rerun readiness before real publishing. Python packages support built-in PyPI publishing with `uv build` and `uv publish`. Go packages publish by creating VCS tags (`v1.2.3` for root modules, `path/v1.2.3` for submodules) and checking visibility through the Go module proxy. Keep `mode = "external"` for private registries or custom publication flows.

<!-- {@projectCapabilityMatrix} -->

| Capability                                                                     | Current status                                                                                                                             |
| ------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Multi-ecosystem discovery                                                      | Cargo, npm/pnpm/Bun, Deno, Dart, Flutter, Python, Go                                                                                       |
| Package release planning                                                       | Built in                                                                                                                                   |
| Grouped/shared versioning                                                      | Built in                                                                                                                                   |
| Internal dependency version synchronization                                    | All supported ecosystems via `monochange versions`; release planning also updates supported ecosystems during releases                     |
| Dry-run release diff previews                                                  | Built in via `monochange step prepare-release --dry-run --diff`; configured workflows may expose `monochange run release --dry-run --diff` |
| Durable release history and post-merge tagging                                 | Built in via `ReleaseRecord`, `monochange step release-record`, `monochange step tag-release`, and `monochange step retarget-release`      |
| Hosted provider releases                                                       | GitHub, GitLab, Gitea, Forgejo                                                                                                             |
| Hosted release requests                                                        | GitHub, GitLab, Gitea, Forgejo                                                                                                             |
| Python release planning                                                        | Built in for discovery, version rewrites, dependency rewrites, lockfile command inference, and PyPI publishing                             |
| Go release planning                                                            | Built in for `go.mod` discovery, dependency rewrites, `go mod tidy` inference, and Go proxy tag publishing                                 |
| Built-in registry publishing                                                   | `crates.io`, `npm`, `jsr`, `pub.dev`, `pypi`, Go proxy tags; use external mode for custom registries                                       |
| GitHub npm trusted-publishing diagnostics                                      | Built in; registry-side enrollment stays manual or external                                                                                |
| GitHub trusted-publishing guidance for `crates.io`, `jsr`, `pub.dev`, and PyPI | Built in, but manual registry enrollment is still required                                                                                 |
| GitLab trusted-publishing auto-derivation                                      | Not built in                                                                                                                               |
| Release-retarget sync for hosted releases                                      | GitHub first                                                                                                                               |

<!-- {/projectCapabilityMatrix} -->

<!-- {@projectGitHubAutomationOverview} -->

monochange can promote one prepared release into several source-provider automation flows without changing the underlying release-plan model.

- `monochange run release --dry-run --format json` refreshes the cached manifest and shows downstream automation data, including authored changesets plus linked release context metadata
- `monochange step publish-release --dry-run --format json` previews provider release payloads before publishing
- `monochange step open-release-request --dry-run --format json` previews the release branch, commit, and release-request body
- when `[source.pull_requests].verified_commits = true` and `OpenReleaseRequest` runs on GitHub Actions for the configured GitHub repository, the GitHub provider pushes a normal release branch commit first, then attempts to replace it with a Git Database API commit that GitHub reports as verified; if verification or the API update fails, the normal pushed commit remains in place
- `monochange step release-record --from <tag>` inspects the durable release declaration stored in the release commit body
- `monochange step tag-release --from HEAD --dry-run --format json` previews the post-merge release tag set declared by that durable record
- `monochange step retarget-release --from <tag> --dry-run` previews a release-retarget plan before mutating tags
- changelog templates can render linked change owners, review requests, commits, and closed issues through `{{ context }}` or fine-grained metadata variables
- `monochange step affected-packages --format json --verify --changed-paths ...` evaluates pull-request changeset policy from CI-supplied paths and labels without requiring a config-defined wrapper command
- `monochange step diagnose-changesets --format json` shows all discovered changeset context or restricts to explicit inputs

<!-- {/projectGitHubAutomationOverview} -->

<!-- {@projectTagReleaseJsonTagsMap} -->

When a post-merge workflow needs to trigger follow-up release work, prefer `monochange step tag-release --from HEAD --format json` and read the release tag by package or group id from the top-level `tags` object:

```json
{
	"tags": {
		"main": "v1.2.3",
		"sdk": "sdk/v1.2.3"
	}
}
```

`name/version` examples such as `sdk/v1.2.3` correspond to a tag template like `{{ name }}/v{{ version }}`.

The `tags` object is intentionally flat because package ids and group ids share the same monochange namespace. A workspace cannot have both a package and a group with the same id, so workflows do not need separate `tags.packages` and `tags.groups` branches or prefixed lookup keys. This makes automation stable and explicit: use `.tags.<id>` for the package or group whose release should drive the next step.

A package or group might not be released in a particular release commit. Handle that by checking whether `tags` has an entry for the id you care about. If there is no tag attached to that id, you can assume that release did not include that package or group and skip that follow-up workflow.

For example, a repository with `[group.main]` can trigger a downstream GitHub release workflow from the main group tag with:

```bash
monochange step tag-release --from HEAD --format json >/tmp/tag-report.json
tag="$(jq -r '.tags.main // empty' /tmp/tag-report.json)"

if [ -z "$tag" ]; then
  echo "No main group tag found in tag-report.json, skipping release trigger"
  exit 0
fi

gh workflow run release.yml --ref "$tag" -f tag="$tag"
```

Avoid indexing `tagResults[0]` for workflow control. `tagResults` remains the audit log of tag operations, while `tags` is the stable id-addressable map for automation.

<!-- {/projectTagReleaseJsonTagsMap} -->

<!-- {@repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
monochange step validate
monochange step discover --format json
monochange run change --package monochange --bump minor --reason "add release planning"
monochange step diagnose-changesets --format json
monochange run release --dry-run --format json
monochange step publish-release --dry-run --format json
monochange step open-release-request --dry-run --format json
monochange step release-record --from v1.2.3
monochange step tag-release --from HEAD --dry-run --format json
monochange step publish-readiness --from HEAD --output .monochange/readiness.json
monochange step placeholder-publish
monochange step publish-readiness --from HEAD --output .monochange/readiness.json
monochange step plan-publish-rate-limits --readiness .monochange/readiness.json --format json
monochange step publish-packages --output .monochange/publish-result.json
monochange step retarget-release --from v1.2.3 --target HEAD --dry-run
monochange run release
```

<!-- {/repoDevEnvironmentSetupCode} -->

<!-- {@repoCommonDevelopmentCommands} -->

```bash
monochange --help
docs:check      # verify mdt shared-doc synchronization
docs:update     # synchronize shared docs via mdt update
schema:check    # verify committed JSON schemas are current
schema:update   # regenerate schema assets from source
monochange step validate
lint:all
test:all
coverage:all
coverage:patch
build:all
build:book
```

<!-- {/repoCommonDevelopmentCommands} -->

<!-- {@contributingCoreCommands} -->

```bash
monochange --help
docs:check
docs:update
monochange step validate
monochange run change --package monochange --bump patch --reason "describe the change"
lint:all
test:all
coverage:all
coverage:patch
build:all
build:book
```

<!-- {/contributingCoreCommands} -->

<!-- {@projectSetupConfig} -->

```toml
[defaults]
parent_bump = "patch"
warn_on_group_mismatch = true
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[changelog]
templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
]

[package.sdk-core]
path = "crates/sdk_core"
[package.sdk-core.changelog.types]
security = { bump = "patch", section = "Security" }

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[package.mobile-sdk]
path = "packages/mobile-sdk"
type = "dart"

[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
tag = true
release = true
version_format = "primary"

[group.sdk.changelog]
path = "docs/sdk-changelog.md"
format = "monochange"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases]
source = "monochange"

[source.pull_requests]
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[changesets.affected]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**", "packages/**", "npm/**", "skills/**"]
ignored_paths = [
	"docs/**",
	"specs/**",
	"readme.md",
	"CONTRIBUTING.md",
	"license",
]

name = "production"
trigger = "release_pr_merge"
release_targets = ["sdk"]
requires = ["main"]

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "json-min"]
default = "text"

[[cli.discover.steps]]
name = "discover packages"
type = "Discover"
inputs = ["format"]

[cli.change]
help_text = "Create a change file for one or more packages"

[[cli.change.inputs]]
name = "interactive"
type = "boolean"
short = "i"

[[cli.change.inputs]]
name = "package"
type = "string_list"

[[cli.change.inputs]]
name = "bump"
type = "choice"
choices = ["none", "patch", "minor", "major"]
default = "patch"

[[cli.change.inputs]]
name = "version"
type = "string"

[[cli.change.inputs]]
name = "reason"
type = "string"

[[cli.change.inputs]]
name = "type"
type = "string"

[[cli.change.inputs]]
name = "details"
type = "string"

[[cli.change.inputs]]
name = "output"
type = "path"

[[cli.change.steps]]
name = "create change file"
type = "CreateChangeFile"
inputs = ["interactive", "package", "bump", "version", "type", "reason", "details", "output"]

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "json-min"]
default = "text"

[[cli.release.steps]]
name = "prepare release"
type = "PrepareRelease"
inputs = ["format"]

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "json-min"]
default = "text"

[[cli.publish-release.steps]]
name = "prepare release"
type = "PrepareRelease"
inputs = ["format"]

[[cli.publish-release.steps]]
name = "publish release"
type = "PublishRelease"
inputs = ["format"]

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "json-min"]
default = "text"

[[cli.release-pr.steps]]
name = "prepare release"
type = "PrepareRelease"
inputs = ["format"]

[[cli.release-pr.steps]]
name = "open release request"
type = "OpenReleaseRequest"
inputs = ["format"]

[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "json-min"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.steps]]
name = "evaluate affected packages"
type = "AffectedPackages"
inputs = ["format", "changed_paths", "label"]
```

<!-- {/projectSetupConfig} -->

<!-- {@projectSetupConfigNote} -->

This guide shows the preferred package/group configuration model together with an expanded CLI command surface.

<!-- {/projectSetupConfigNote} -->

<!-- {@projectDiscoverCommand} -->

```bash
monochange step discover --format json
```

<!-- {/projectDiscoverCommand} -->

<!-- {@projectDryRunCommand} -->

```bash
monochange run release --dry-run --format json
```

<!-- {/projectDryRunCommand} -->

<!-- {@projectPlanCommand} -->

```bash
monochange run release --dry-run --format json
```

<!-- {/projectPlanCommand} -->

<!-- {@projectReleaseCommand} -->

```bash
monochange run release
```

<!-- {/projectReleaseCommand} -->

<!-- {@projectValidationCommands} -->

```bash
lint:all
test:all
build:all
build:book
```

<!-- {/projectValidationCommands} -->

<!-- {@projectDiscoveryOutputIncludes} -->

- normalized package records
- dependency edges
- release groups derived from configured groups
- warnings

<!-- {/projectDiscoveryOutputIncludes} -->

<!-- {@projectReleaseOutputIncludes} -->

- per-package bump decisions
- synchronized group outcomes
- compatibility evidence
- warnings and unresolved items
- optional `fileDiffs` previews when you request `--diff`

<!-- {/projectReleaseOutputIncludes} -->

<!-- {@projectCoreWorkflow} -->

Generate a starter config from the packages monochange detects:

```bash
monochange init
```

`monochange init` writes an annotated, minimal `monochange.toml` without default `[cli.*]` workflow aliases. The binary exposes immutable `monochange step *` commands for every built-in step when you need a direct, config-free entry point; add `[cli.*]` tables only for repository-specific named workflows.

For automated CI setup, include the `--provider` flag:

```bash
monochange init --provider github
```

This configures the `[source]` section and creates GitHub Actions workflows for changeset policy and release automation. It intentionally does not add `[cli.*]` workflow commands.

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
