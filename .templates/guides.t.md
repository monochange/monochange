<!-- {@discoverySupportedSources} -->

- Cargo workspaces and standalone crates
- npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages
- Deno workspaces and standalone `deno.json` / `deno.jsonc` packages
- Dart and Flutter workspaces plus standalone `pubspec.yaml` packages
- Python uv workspaces, Poetry projects, and standalone `pyproject.toml` packages
- Go modules discovered from standalone `go.mod` files

<!-- {/discoverySupportedSources} -->

<!-- {@discoveryKeyBehaviors} -->

- native workspace globs are expanded by each ecosystem adapter
- dependency names are normalized into one graph
- package ids and manifest paths in CLI output are rendered relative to the repository root for deterministic automation
- gitignored paths and nested git worktrees are skipped during discovery
- version-group assignments are attached after discovery
- unmatched group members (declared in config but not found during discovery) produce warnings
- unresolvable group members (invalid package IDs in `group.packages`) produce errors during configuration loading
- discovery scans all supported ecosystems regardless of `[ecosystems.*]` toggles in `monochange.toml`

<!-- {/discoveryKeyBehaviors} -->

<!-- {@initProviderFeature} -->

The `--provider` flag supports `github`, `gitlab`, and `gitea`. When provided, `monochange init`:

1. **Configures the `[source]` section** - adds provider-specific settings for releases and pull/merge requests
2. **Generates provider CLI commands** - includes `commit-release` and `release-pr` commands in `monochange.toml`
3. **Creates workflow files** (GitHub only) - writes `.github/workflows/release.yml` and `.github/workflows/changeset-policy.yml`
4. **Auto-detects owner/repo** - parses `git remote get-url origin` to pre-populate `[source]`

Example generated configuration with `--provider github`:

```toml
[source]
provider = "github"
owner = "ifiokjr" # auto-detected from git remote
repo = "monochange" # auto-detected from git remote

[source.releases]
enabled = true
draft = false
prerelease = false
source = "monochange"
branches = ["main", "release/*"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
changeset_context_timeout_seconds = 120

[source.pull_requests]
enabled = true
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[cli.commit-release]
help_text = "Prepare a release and create a release commit"

[[cli.commit-release.steps]]
type = "PrepareRelease"
name = "plan release"

[[cli.commit-release.steps]]
type = "CommitRelease"
name = "create release commit"

[cli.release-pr]
help_text = "Prepare a release and open a release pull request"

[[cli.release-pr.steps]]
type = "PrepareRelease"
name = "plan release"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
name = "open release PR"
```

The GitHub Actions workflows enable:

- **Release automation** - `release.yml` refreshes the release PR on normal `main` pushes, then tags and publishes when the merged release commit lands on `main`
- **Changeset policy enforcement** - `changeset-policy.yml` validates PRs have required changeset coverage

For GitLab and Gitea, the `[source]` section is configured but workflows are not generated (use their respective CI configuration files).

<!-- {/initProviderFeature} -->

<!-- {@initProviderQuickStart} -->

```bash
# Initialize with GitHub automation pre-configured
monochange init --provider github

# The generated monochange.toml includes:
# - [source] section with GitHub releases and pull request settings
# - CLI commands for commit-release and release-pr
# - GitHub Actions workflows in .github/workflows/
```

This single command generates:

1. **Complete source configuration** - `[source]`, `[source.releases]`, and `[source.pull_requests]` sections
2. **Automation CLI commands** - `commit-release` and `release-pr` commands ready to use
3. **GitHub Actions workflows** - `release.yml` and `changeset-policy.yml` for CI/CD
4. **Auto-detected repository info** - parses your git remote to pre-fill owner and repo

<!-- {/initProviderQuickStart} -->

<!-- {@configurationDefaultsSnippet} -->

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
strict_version_conflicts = false
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"
```

<!-- {/configurationDefaultsSnippet} -->

<!-- {@configurationVersionGroupsSnippet} -->

```toml
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[package.sdk-core]
path = "crates/sdk_core"
versioned_files = [
	"Cargo.toml",
	{ path = "crates/sdk_core/extra.toml", type = "cargo" },
]
tag = false
release = false
version_format = "namespaced"

[package.sdk-core.changelog]
path = "crates/sdk_core/CHANGELOG.md"
format = "monochange"
```

<!-- {/configurationVersionGroupsSnippet} -->

<!-- {@configurationRegexVersionedFilesSnippet} -->

Regex entries let you version-stamp any plain-text file, such as README badges, download links, or install scripts, without needing an ecosystem-specific parser. The regex must contain a named `version` capture group; monochange replaces the captured substring with the new version while preserving the surrounding text.

```toml
[package.core]
path = "crates/core"
versioned_files = [
	# update a download link in the README
	{ path = "README.md", regex = 'https://example\.com/download/v(?<version>\d+\.\d+\.\d+)\.tgz' },
	# update a version badge
	{ path = "README.md", regex = 'img\.shields\.io/badge/version-(?<version>\d+\.\d+\.\d+)-blue' },
]

[group.sdk]
packages = ["core", "cli"]
versioned_files = [
	# update the install script across all packages (glob pattern)
	{ path = "**/install.sh", regex = 'SDK_VERSION="(?<version>\d+\.\d+\.\d+)"' },
]

[ecosystems.cargo]
versioned_files = [
	# update a workspace-wide version constant
	{ path = "crates/constants/src/lib.rs", regex = 'pub const VERSION: &str = "(?<version>\d+\.\d+\.\d+)"' },
]
```

Key rules:

- `regex` entries cannot set `type`, `prefix`, `fields`, or `name`: they operate on raw text
- the regex must include a `(?<version>...)` named capture group
- the `path` field supports glob patterns (e.g. `**/README.md`)
- regex entries work on packages, groups, and ecosystem-level `versioned_files`

<!-- {/configurationRegexVersionedFilesSnippet} -->

<!-- {@configurationPackageOverridesSnippet} -->

When `[defaults].package_type` is set, package entries may omit an explicit `type`.

monochange supports two changelog formats:

- `monochange` keeps the current heading-and-bullets layout
- `keep_a_changelog` renders section headings such as `### Features`, `### Fixes`, and `### Breaking changes`

Defaults can set a repository-wide changelog path pattern and format, while package and group changelog tables can override either field.

Use `[changelog.style]` to tune rendered release-note shape. `metadata_style` accepts `inline` (the default), `blockquote`, `plain`, or `omit`. The inline style renders owner, review request, and issue metadata as one `·`-separated paragraph; when a PR/MR link is available, commit links are omitted because the review link already identifies the change.

```toml
[changelog.style]
metadata_style = "inline"
```

You can also customize release-note rendering with a workspace-wide `[release_notes]` table plus per-package or per-group `extra_changelog_sections` definitions.

Supported template variables include:

| Variable                         | Meaning                                                               | Notes                                                                                                      |
| -------------------------------- | --------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `{{ summary }}`                  | rendered release-note summary heading                                 | always available                                                                                           |
| `{{ details }}`                  | optional long-form details body                                       | omitted when the changeset has no details                                                                  |
| `{{ package }}`                  | owning package id for the rendered entry                              | useful in shared templates                                                                                 |
| `{{ version }}`                  | release version for the current target                                | package or group version                                                                                   |
| `{{ target_id }}`                | release target id                                                     | package id or group id                                                                                     |
| `{{ bump }}`                     | resolved bump severity                                                | `none`, `patch`, `minor`, or `major`                                                                       |
| `{{ type }}`                     | changeset note type                                                   | e.g. `feature`, `fix`, `security`; omitted when absent                                                     |
| `{{ context }}`                  | compact default metadata block                                        | preferred rendered block for human-readable notes                                                          |
| `{{ changeset_path }}`           | source `.changeset/*.md` path                                         | tracked in manifests and still available for custom templates, but not shown by default in `{{ context }}` |
| `{{ change_owner }}`             | plain-text hosted actor label                                         | usually something like `@ifiokjr`                                                                          |
| `{{ change_owner_link }}`        | markdown link to the hosted actor                                     | falls back to plain text when no URL is available                                                          |
| `{{ review_request }}`           | plain-text PR/MR label                                                | e.g. `PR #31` or `MR !42`                                                                                  |
| `{{ review_request_link }}`      | markdown link to the PR/MR                                            | falls back to plain text when no URL is available                                                          |
| `{{ introduced_commit }}`        | short SHA for the commit that first introduced the changeset          | plain text only                                                                                            |
| `{{ introduced_commit_link }}`   | markdown link to the introducing commit                               | preferred for changelog output                                                                             |
| `{{ last_updated_commit }}`      | short SHA for the most recent commit that changed the changeset       | only populated when different from `{{ introduced_commit }}`                                               |
| `{{ last_updated_commit_link }}` | markdown link to the most recent commit that changed the changeset    | only populated when different from `{{ introduced_commit }}`                                               |
| `{{ closed_issues }}`            | plain-text list of issues closed by the linked review request         | typically `#12, #18`                                                                                       |
| `{{ closed_issue_links }}`       | markdown links to issues closed by the linked review request          | preferred for changelog output                                                                             |
| `{{ related_issues }}`           | plain-text list of related issues that were referenced but not closed | host support may vary                                                                                      |
| `{{ related_issue_links }}`      | markdown links to related issues that were referenced but not closed  | host support may vary                                                                                      |

The `*_link` variants render markdown links when the hosting provider exposes URLs. By default `{{ context }}` renders the highest-value metadata for readers: owner, review request, introduced commit, last updated commit when different, and linked issues. It does not expose the transient `.changeset/*.md` path unless you explicitly reference `{{ changeset_path }}` in your template.

<!-- {/configurationPackageOverridesSnippet} -->

<!-- {@configurationBumpPropagationSnippet} -->

Packages, groups, and the `defaults` section declare what a target's own changes mean for the packages that depend on them via `bump_propagation`:

```toml
[defaults]
# Workspace-wide fallback for dependents with no package or group declaration.
bump_propagation = "inherit"
bump_propagation_max = "major"

[package.core]
# Dependents match this package's release severity, never exceeding `minor`.
bump_propagation = "inherit"
bump_propagation_max = "minor"

[package.tooling]
# Dependents always receive at least a minor bump from this package's changes.
bump_propagation = "minor"

[package.leaf]
# Dependents never release because of this package.
bump_propagation = "none"

[group.sdk]
# A group declaration applies to members that declare nothing.
packages = ["core"]
bump_propagation = "major"
```

- `inherit` matches the target's own release severity: a breaking change in the package means breaking changes for its dependents. `bump_propagation_max` clamps the inherited severity (only valid with `inherit`).
- A fixed severity (`none`, `patch`, `minor`, `major`) is a floor: dependents release at least that severity whenever the package releases. `none` disables dependency propagation entirely.
- Declarations resolve most-specific-first: a package declaration overrides its group's declaration, which overrides `[defaults].bump_propagation`. Targets matching no declaration fall back to the legacy `[defaults].parent_bump` floor. Semantic compatibility evidence can still escalate beyond the declared floor.

<!-- {/configurationBumpPropagationSnippet} -->

<!-- {@releaseBumpPropagationReadme} -->

Dependency-driven version bumps are declarative too. Packages and groups declare what their own changes mean for dependents with `bump_propagation`: `"inherit"` matches the source's bump (optionally clamped by `bump_propagation_max`), and fixed severities (`none`/`patch`/`minor`/`major`) act as floors. Resolution is most-specific-first (package beats group beats defaults), so one kit core package can bring breaking changes to every dependent the moment upstream does — without hand-authoring dependent bumps.

<!-- {/releaseBumpPropagationReadme} -->

<!-- {@configurationWorkflowsSnippet} -->

```toml
[release_notes]
change_templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
]

[package.core]
path = "crates/core"
extra_changelog_sections = [
	{ name = "Security", types = ["security"], default_bump = "patch" },
]

[cli.discover]
help_text = "Discover packages across supported ecosystems"

[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.discover.steps]]
name = "discover packages"
type = "Discover"
inputs = ["format"]

[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
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
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
name = "prepare release"
type = "PrepareRelease"
inputs = ["format"]

[[cli.publish-release.steps]]
name = "publish release"
type = "PublishRelease"
inputs = ["format"]

[[cli.publish-release.steps]]
name = "comment released issues"
type = "CommentReleasedIssues"

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
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
choices = ["text", "json"]
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

<!-- {/configurationWorkflowsSnippet} -->

<!-- {@configurationWorkflowVariables} -->

- built-in command variables are available directly as `{{ version }}`, `{{ group_version }}`, `{{ released_packages }}`, `{{ changed_files }}`, and `{{ changesets }}`
- command templates can read CLI inputs through `{{ inputs.name }}`
- every step can override the inputs it receives with `inputs = { ... }`; direct references like `"{{ inputs.labels }}"` preserve list and boolean values when rebinding to built-in steps
- built-in commands already attach descriptive step `name` labels such as `prepare release` and `publish release`; keep or replace those labels when you want progress output to stay readable
- custom command variables become available when `variables` is present: map your own names to variables such as `version`, `group_version`, `released_packages`, `changed_files`, and `changesets`
- `always_run = true` on any step causes it to run even when a previous step has failed, which is useful for cleanup, notification, or dry-run preview steps
- `update_release_json = true` on a `CommitRelease` step allows the step to create or overwrite the release record file when it is missing or differs from the expected content; the default (`false`) treats a missing or mismatched record as an error
- `dry_run_command` on a `Command` step replaces `command` only when the CLI command is run with `--dry-run`
- `dry_run = true` on a `[cli.<command>]` table forces the entire command to run in dry-run mode even when the user does not pass `--dry-run`
- `shell = true` runs the command through the current shell; the default mode runs the executable directly after shell-style splitting

<!-- {/configurationWorkflowVariables} -->

<!-- {@configurationGitHubSnippet} -->

The `[source]` section configures provider integration for releases, pull requests, and changeset enforcement.

For self-hosted instances, set `api_url` or `host` to your server's URL. These fields **must** use `https://`; insecure `http://` schemes are rejected because API tokens would be transmitted in cleartext.

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"
# api_url = "https://github.company.com/api/v3"  # optional: for GitHub Enterprise

[source.releases]
enabled = true
draft = false
prerelease = false
source = "monochange"
branches = ["main", "release/*"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
changeset_context_timeout_seconds = 120

[source.pull_requests]
enabled = true
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
```

<!-- {/configurationGitHubSnippet} -->

<!-- {@configurationEcosystemSettingsSnippet} -->

```toml
[ecosystems.cargo]
enabled = true
roots = ["crates/*"]
exclude = ["crates/experimental/*"]
lockfile_commands = [{ command = "cargo generate-lockfile" }]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
exclude = ["packages/legacy/*"]
dependency_version_prefix = "^"
versioned_files = ["**/packages/*/package.json"]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
]

[ecosystems.deno]
enabled = true
# Deno has no inferred lockfile command.

[ecosystems.dart]
enabled = true
lockfile_commands = [{ command = "flutter pub get", cwd = "packages/mobile" }]

[ecosystems.python]
enabled = true
lockfile_commands = [{ command = "uv lock" }]

[ecosystems.go]
enabled = true
# monochange infers `go mod tidy` for go.mod / go.sum refreshes.
lockfile_commands = [{ command = "go mod tidy" }]
```

<!-- {/configurationEcosystemSettingsSnippet} -->

<!-- {@configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured ids.

Prefer package ids when a leaf package changed. That keeps the authored change as specific as possible, and monochange will still propagate bumps to dependents and synchronize any configured groups automatically.

Use a group id only when the change is intentionally owned by the whole group and should read that way in release output.

<!-- {/configurationPackageReferenceRules} -->

<!-- {@configurationCurrentStatus} -->

Implementation notes:

- `defaults.include_private` is parsed, but discovery behavior is still centered on the supported fixture-driven CLI commands documented here
- `[ecosystems.*].enabled/roots/exclude` are parsed, but discovery still scans all supported ecosystems regardless of those settings
- `defaults.strict_version_conflicts` controls whether conflicting explicit `version` entries across changesets warn-and-pick-highest (default) or fail planning outright
- source automation expects `[source]` with provider release settings and release branch policy under `[source.releases]`, pull request settings under `[source.pull_requests]`, and affected-package policy settings under `[changesets.affected]`; GitHub remains the default provider
- live GitHub release and release-request publishing uses `octocrab` with `GITHUB_TOKEN` / `GH_TOKEN`, falling back to the authenticated GitHub CLI credential via `gh auth token` when neither variable is set; GitLab and Gitea use direct HTTP APIs
- release-request publishing still uses local `git` for branch, commit, and push operations before provider API updates when not in dry-run mode
- changeset policy commands apply only to the GitHub provider and expect `[changesets.affected]`, a `changed_paths` command input, and reusable diagnostics for GitHub Actions consumption
- supported `[[cli.<command>.steps]]` types are `Config`, `Validate`, `Discover`, `DisplayVersions`, `CreateChangeFile`, `PrepareRelease`, `CommitRelease`, `VerifyReleaseBranch`, `PublishRelease`, `PlaceholderPublish`, `PublishPackages`, `PlanPublishRateLimits`, `OpenReleaseRequest`, `CommentReleasedIssues`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`, `ReleaseRecord`, `PublishReadiness`, `TagRelease`, and `Command`
- see the [CLI step reference](../reference/cli-steps/00-index.md) for detailed per-step guidance, prerequisites, and composition examples

<!-- {/configurationCurrentStatus} -->

<!-- {@versionGroupsExample} -->

```toml
[package.sdk-core]
path = "cargo/sdk-core"
type = "cargo"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[group.sdk]
packages = ["sdk-core", "web-sdk"]
tag = true
release = true
version_format = "primary"
```

Groups can also use `version_format = "namespaced"` or a custom tag template such as `version_format = "{{ name }}/v{{ version }}"`. Custom formats support `{{ name }}`, `{{ version }}`, and `{{ ecosystem }}`, must include `{{ version }}`, and must render unique valid Git tag names.

<!-- {/versionGroupsExample} -->

<!-- {@versionGroupsBehavior} -->

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- the group owns outward release identity
- member package changelogs can still be updated individually
- group changelog and group `versioned_files` can also be updated
- grouped packages can use `empty_update_message` when their own changelog needs a version-only update with no direct notes
- dependents of newly synced members still receive propagated parent bumps
- unmatched members (not found during discovery) produce warnings; unresolvable members (invalid IDs) produce errors
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

<!-- {@releaseChangesAddCommand} -->

```bash
monochange run change --package sdk-core --bump minor --reason "public API addition"
monochange run change --package sdk-core --bump patch --type security --reason "rotate signing keys" --details "Roll the signing key before the release window closes."
monochange run change --package sdk-core --bump none --type docs --reason "clarify migration guidance" --output .changeset/sdk-core-docs.md
monochange run change --package sdk-core --bump major --version 2.0.0 --reason "break the public API" --output .changeset/sdk-core-major.md
```

Or use interactive mode to select packages, bumps, and options from a guided wizard:

```bash
monochange run change -i
```

Interactive mode automatically prevents conflicting selections (a group and one of its members) and lets you pick per-package bumps and optional explicit versions.

<!-- {/releaseChangesAddCommand} -->

<!-- {@releaseManualChangesetExample} -->

```markdown
---
sdk-core:
  bump: patch
  type: security
---

# rotate signing keys

Roll the signing key before the release window closes.
```

<!-- {/releaseManualChangesetExample} -->

<!-- {@releaseExplicitVersionChangesetExample} -->

Prefer the inline form: write a configured change type as the target value when its default bump is what you want (`sdk-core: docs` for a documentation-only change, `sdk-core: fix` for a patch fix). Use scalar bumps (`sdk-core: minor`) for plain bumps without a custom type. Use the object syntax only when you need to pin an exact version, combine `bump`, `version`, and `type`, or override a type's default bump (for example `docs` with a `patch` bump):

```markdown
---
sdk-core:
  bump: major
  version: "2.0.0"
---

# promote to stable
```

When `version` is provided without `bump`, the bump is inferred from the current version. If the package belongs to a version group, the explicit version propagates to the whole group. Overriding a type's default bump is possible but should be avoided unless the user explicitly asks for that version behavior.

<!-- {/releaseExplicitVersionChangesetExample} -->

<!-- {@releasePlanningRules} -->

- `monochange run change` defaults `--bump` to `patch`; use `--bump none` when you want a type-only or version-only entry, and pass `--version` to pin an explicit release version
- markdown change files use package/group ids as the only top-level frontmatter keys, with scalar shorthand for `none`/`patch`/`minor`/`major` or configured change types, plus object syntax for `bump`, `version`, `type`, and `caused_by`
- when `version` is given without `bump`, the bump is inferred by comparing the current and target versions
- explicit versions from grouped members propagate to the group version; conflicts take the highest semver or fail when `defaults.strict_version_conflicts = true`
- prefer package ids over group ids in authored changesets when possible; direct package changes still propagate to dependents and synchronize configured groups
- optional change `type` values can route entries into custom changelog sections, and configured section `default_bump` values let scalar type shorthand imply the desired semver behavior
- `caused_by` references package or group ids and suppresses only the matching dependency-propagation records; use object syntax whenever you need it
- `monochange run change` accepts repeated `--caused-by <id>` flags, and `--bump none` is the right fit when you want to acknowledge an affected package without forcing a user-facing version bump
- `monochange run change` can write to a deterministic path with `--output ...`
- change templates support detailed multi-line release-note entries through `{{ details }}`, compact metadata blocks through `{{ context }}`, and fine-grained linked metadata like `{{ change_owner_link }}`, `{{ review_request_link }}`, and `{{ closed_issue_links }}`
- dependents resolve their propagation policy most-specific-first: `[[package]].bump_propagation` overrides `group` declarations, which override `[defaults].bump_propagation`, which falls back to the `[defaults].parent_bump` floor; `inherit` matches the source's own severity (optionally clamped by `bump_propagation_max`) and fixed severities act as floors
- computed compatibility evidence can still escalate both the changed crate and its dependents when provider analysis produces it
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata
- release-manifest JSON captures release targets, changelog payloads, authored changesets, linked changeset context metadata, changed files, and the synchronized release plan for downstream automation
- `PublishRelease` reuses the same structured release data to build provider release requests for grouped and package-owned releases
- `OpenReleaseRequest` reuses the same structured release data to render release-request summaries, branch names, and idempotent provider updates
- `CommentReleasedIssues` can use linked changeset context metadata to add follow-up comments to closed issues after a release is published
- `AffectedPackages` evaluates changed paths, skip labels, and changed `.changeset/*.md` files into reusable pass/skip/fail diagnostics and optional failure comments
- CLI text and JSON output render workspace paths relative to the repository root for stable snapshots and automation

<!-- {/releasePlanningRules} -->

<!-- {@releaseWorkflowBehavior} -->

`monochange run release` is a config-driven workflow command only when your repository defines a `[cli.release]` table. `monochange init` writes a minimal starter config and does not seed default workflow aliases, so use the immutable `monochange step prepare-release` command unless you add your own named workflow.

The binary no longer ships a hidden default workflow set for commands such as `discover`, `change`, `release`, `affected`, `diagnostics`, `repair-release`, `publish`, or `publish-plan`. Those names exist under `monochange run <name>` only when your config defines them. If a repository has not opted into a named workflow, use the immutable step command instead, for example `monochange step discover`, `monochange step create-change-file`, `monochange step prepare-release`, `monochange step affected-packages`, `monochange step diagnose-changesets`, `monochange step retarget-release`, `monochange step publish-readiness`, or `monochange step plan-publish-rate-limits`.

`monochange step validate` is the immutable built-in step command for normal preflight checks. Do not define `[cli.validate]` or `[cli.step]` in `monochange.toml`; those names are reserved for built-in commands.

Configured workflows like `monochange run commit-release` combine `PrepareRelease` with later stateful steps such as `CommitRelease`. Provider request workflows such as `monochange run release-pr` can add `OpenReleaseRequest`. Keep both as explicit `[cli.*]` workflow commands when you want a durable, named release process.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- renders changelog files through structured release notes using the configured `monochange` or `keep_a_changelog` format
- groups release notes into default `Breaking changes`, `Features`, `Fixes`, and `Notes` sections, with package/group overrides available through `extra_changelog_sections`
- applies workspace-wide release-note templates from `[release_notes].change_templates`
- refreshes the cached `.monochange/release-manifest.json` artifact during `PrepareRelease` for downstream automation
- can preview or publish provider releases via `PublishRelease`
- can preview or open/update release requests via `OpenReleaseRequest`
- can comment on released issues via `CommentReleasedIssues`
- can evaluate pull-request changeset policy via `AffectedPackages` using changed paths and labels supplied by CI
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run` except for explicitly requested outputs such as a rendered release manifest or release preview

A GitHub Actions check can pass changed paths and labels directly into a policy workflow, for example:

<!-- {/releaseWorkflowBehavior} -->

<!-- {@changesetPolicyGitHubActionWorkflow} -->

```yaml
name: changeset-policy

on:
  pull_request:
    types:
      - opened
      - synchronize
      - reopened
      - labeled
      - unlabeled

concurrency:
  group: changeset-policy-${{ github.event.pull_request.number || github.ref }}
  cancel-in-progress: true

jobs:
  check:
    timeout-minutes: 60
    runs-on: ubuntu-latest
    permissions:
      contents: read
      pull-requests: read
    steps:
      - name: checkout repository
        uses: actions/checkout@v6

      - name: setup
        uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: collect changed files
        id: changed
        uses: tj-actions/changed-files@v46

      - name: run changeset policy
        env:
          PR_LABELS_JSON: ${{ toJson(github.event.pull_request.labels.*.name) }}
          CHANGED_FILES: ${{ steps.changed.outputs.all_changed_files }}
        shell: bash
        run: |
          set -euo pipefail

          mapfile -t labels < <(jq -r '.[]' <<<"$PR_LABELS_JSON")
          args=(step affected-packages --format json --verify)

          for path in $CHANGED_FILES; do
            args+=(--changed-paths "$path")
          done

          for label in "${labels[@]}"; do
            args+=(--label "$label")
          done

          devenv shell -- monochange "${args[@]}" | tee policy.raw
          awk 'BEGIN { capture = 0 } /^\{/ { capture = 1 } capture { print }' policy.raw > policy.json
          jq -e '.status != "failed"' policy.json >/dev/null
```

<!-- {/changesetPolicyGitHubActionWorkflow} -->

<!-- {@githubAutomationOverview} -->

monochange keeps source-provider automation layered on top of the same `PrepareRelease` result used for normal release planning.

That means one set of `.changeset/*.md` inputs can drive all of these commands and automation flows consistently:

- `monochange step prepare-release --dry-run --format json` refreshes the cached manifest and shows the downstream automation payload
- `monochange step publish-release` previews or publishes provider releases from the structured release notes
- `monochange step open-release-request` previews or opens an idempotent provider release request; when `[source.pull_requests].verified_commits = true` and the step runs on GitHub Actions for the configured repository, the GitHub provider pushes a normal release branch commit as a fallback and then only moves the branch to a Git Database API replacement commit when GitHub reports that replacement as verified
- `monochange step affected-packages` evaluates pull-request changeset policy from CI-supplied changed paths and labels without requiring a config-defined wrapper command

<!-- {/githubAutomationOverview} -->

<!-- {@githubAutomationWorkflowCommands} -->

```bash
monochange step prepare-release --dry-run --format json
monochange step publish-release --dry-run --format json
monochange step open-release-request --dry-run --format json
monochange step affected-packages --format json --verify --changed-paths crates/monochange/src/lib.rs
```

<!-- {/githubAutomationWorkflowCommands} -->

<!-- {@githubAutomationReleaseConfigExample} -->

```toml
[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"

[release_notes]
change_templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ context }}",
	"#### {{ summary }}\n\n{{ details }}",
	"- {{ summary }}",
]

[group.main.changelog]
path = "changelog.md"
format = "monochange"

[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases]
enabled = true
source = "monochange"

[source.releases]
branches = ["main"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
changeset_context_timeout_seconds = 120

[source.pull_requests]
enabled = true
branch_prefix = "monochange/release"
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]
auto_merge = false

[cli.publish-release]
help_text = "Prepare a release and publish provider releases"

[[cli.publish-release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.publish-release.steps]]
type = "PrepareRelease"
inputs = ["format"]

[[cli.publish-release.steps]]
type = "PublishRelease"

[[cli.publish-release.steps]]
type = "CommentReleasedIssues"

[cli.release-pr]
help_text = "Prepare a release and open or update a provider release request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release-pr.steps]]
type = "PrepareRelease"
inputs = ["format"]

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
inputs = ["format"]
```

<!-- {/githubAutomationReleaseConfigExample} -->

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[changesets.affected]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = [
	"crates/**",
	".github/**",
	"Cargo.toml",
	"Cargo.lock",
	"devenv.nix",
	"devenv.yaml",
	"devenv.lock",
	"monochange.toml",
	"codecov.yml",
	"deny.toml",
	"scripts/**",
	"npm/**",
	"skills/**",
]
ignored_paths = [
	".changeset/**",
	"docs/**",
	"specs/**",
	"readme.md",
	"CONTRIBUTING.md",
	"license",
]

name = "docs"
trigger = "release_published"
workflow = "docs-release"
environment = "github-pages"
release_targets = ["main"]
requires = ["main"]
metadata = { site = "github-pages" }

name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

type = "PrepareRelease"

[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.steps]]
type = "AffectedPackages"
```

<!-- {@githubAutomationDogfoodNotes} -->

The monochange repository itself can dogfood this model by:

- declaring `[source]`, `[source.releases]`, and `[source.pull_requests]` in `monochange.toml`
- running a real `changeset-policy` GitHub Actions workflow that shells into `monochange step affected-packages`
- publishing the CLI npm packages from `.github/workflows/publish.yml` with the protected `publisher` environment and `id-token: write`, without `NODE_AUTH_TOKEN` or `NPM_TOKEN`

For monochange's own npm packages, register every package under the GitHub trusted-publishing context `monochange/monochange`, workflow file `publish.yml`, and environment `publisher` before the first tokenless publish:

- `@monochange/cli`
- `@monochange/cli-darwin-arm64`
- `@monochange/cli-darwin-x64`
- `@monochange/cli-linux-arm64-gnu`
- `@monochange/cli-linux-arm64-musl`
- `@monochange/cli-linux-x64-gnu`
- `@monochange/cli-linux-x64-musl`
- `@monochange/cli-win32-arm64-msvc`
- `@monochange/cli-win32-x64-msvc`

After publishing, verify npm provenance from the package page or with npm's provenance metadata for the released version. The expected publisher identity is the `publish.yml` workflow in `monochange/monochange`; a run that lacks npm trusted-publishing setup should fail instead of falling back to a long-lived registry token.

<!-- {/githubAutomationDogfoodNotes} -->

<!-- {@assistantSkillBundleContents} -->

After copying the bundled skill, you get a small documentation set that is designed to load in layers:

- `SKILL.md`: concise entrypoint for agents
- `REFERENCE.md`: broader high-context reference with more examples
- `skills/README.md`: index of focused deep dives
- `skills/adoption.md`: setup-depth questions, migration guidance, and recommendation patterns
- `skills/changesets.md`: changeset authoring and lifecycle guidance
- `skills/commands.md`: built-in command catalog and workflow selection
- `skills/configuration.md`: `monochange.toml` setup and editing guidance
- `skills/linting.md`: `[lints]` presets, `monochange check`, and manifest-focused examples
- `examples/README.md`: condensed scenario examples for quick recommendations

This layout keeps the top-level skill small while still making the richer guidance available when an assistant needs more context.

<!-- {/assistantSkillBundleContents} -->

<!-- {@mcpToolsList} -->

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
- `monochange_classify_changes`: classify API-impacting changes and recommend package bumps
- `monochange_validate_changeset`: validate one changeset against the current semantic diff

<!-- {/mcpToolsList} -->

<!-- {@mcpConfigSnippet} -->

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

<!-- {@assistantRepoGuidance} -->

- Read `monochange.toml` before proposing release workflow changes.
- Run `monochange step validate` before and after release-affecting edits.
- Use `monochange step discover --format json` to inspect package ids, group ownership, and dependency edges.
- Use `monochange step diagnose-changesets --format json` or `monochange_diagnostics` for a structured view of all pending changesets with git and review context.
- Use `monochange_lint_catalog` and `monochange_lint_explain` when you need lint metadata without shelling out.
- Prefer `monochange run change` plus `.changeset/*.md` files over ad hoc release notes.
- Use `monochange step prepare-release --dry-run --format json` before mutating release state.

<!-- {/assistantRepoGuidance} -->

<!-- {@lintingPolicyReference} -->

Use this guide when the task is to configure or explain monochange's **lint rules**.

These are the rules that run through **`monochange check`** and are configured in `monochange.toml` under the top-level **`[lints]`** section. They are separate from Rust compiler or Clippy lints used to develop monochange itself.

This page is the human-readable companion to the live lint catalog. For machine-readable output or to verify the exact catalog in the installed binary, run:

```bash
monochange lint list --format json
monochange lint explain <rule-or-preset-id>
```

## What `monochange check` does

`monochange check` runs two phases:

1. normal workspace validation, similar to `monochange step validate`
2. changeset and manifest lint rules for configured package ecosystems

Common commands:

```bash
monochange check
monochange check --fix
monochange check --format json
monochange lint list
monochange lint explain cargo/recommended
```

Use `--fix` when you want monochange to apply auto-fixes where a rule supports them. Rules that are not autofixable still report diagnostics and suggested remediation.

## Where lint rules live

Configure presets, global rules, and scoped overrides in the top-level `[lints]` section of `monochange.toml`:

```toml
[lints]
use = [
	"changesets/recommended",
	"cargo/recommended",
	"npm/recommended",
	"dart/recommended",
]
exclude = ["fixtures/**"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"
"dart/sdk-constraint-modern" = { level = "warning", minimum = "3.6.0", require_upper_bound = false }
"dart/no-unexpected-dependency-overrides" = { level = "warning", allow_for_private = true, allow_packages = ["app_shell"] }

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

Rule configuration supports two forms:

- simple severity: `"rule-id" = "error"`, `"warning"`, or `"off"`
- detailed config: `{ level = "error", ...rule_specific_options }`

Preset rules provide the baseline. Explicit entries in `[lints.rules]` override that baseline. Scoped rules let a subset of packages be stricter or looser than the workspace default.

## Presets

| Preset                   | What it is for                                                  | Rules enabled                                                                                                                                                                                                                                                                                                                           |
| ------------------------ | --------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `changesets/recommended` | Baseline changeset hygiene.                                     | `changesets/summary = error`, `changesets/prefer-inline = error`                                                                                                                                                                                                                                                                        |
| `cargo/recommended`      | Balanced Cargo manifest policy for most workspaces.             | `cargo/internal-dependency-workspace = error`, `cargo/publishable-dependencies = error`, `cargo/required-package-fields = error`, `cargo/dependency-field-order = warning`, `cargo/sorted-dependencies = warning`, `cargo/unlisted-package-private = warning`                                                                           |
| `cargo/strict`           | Cargo policy with style rules promoted to errors.               | Same as `cargo/recommended`, but `cargo/dependency-field-order` and `cargo/sorted-dependencies` are `error`.                                                                                                                                                                                                                            |
| `npm/recommended`        | Balanced npm-family manifest policy.                            | `npm/workspace-protocol = error`, `npm/no-duplicate-dependencies = error`, `npm/required-package-fields = error`, `npm/root-no-prod-deps = error`, `npm/sorted-dependencies = warning`, `npm/unlisted-package-private = warning`                                                                                                        |
| `npm/strict`             | npm-family policy with dependency sorting promoted to an error. | Same as `npm/recommended`, but `npm/sorted-dependencies` is `error`.                                                                                                                                                                                                                                                                    |
| `dart/recommended`       | Baseline Dart metadata, publishability, and SDK hygiene.        | `dart/sdk-constraint-present = error`, `dart/required-package-fields = error`, `dart/no-git-dependencies-in-published-packages = error`, `dart/unlisted-package-private = error`, `dart/dependency-sorted = warning`                                                                                                                    |
| `dart/strict`            | Dart policy with workspace and Flutter policy rules enforced.   | Everything in `dart/recommended`, plus `dart/sdk-constraint-modern`, `dart/no-unexpected-dependency-overrides`, `dart/internal-path-dependency-policy`, `dart/workspace-internal-version-consistency`, `dart/flutter-package-metadata-consistent`, and `dart/assets-sorted` as errors; `dart/dependency-sorted` is promoted to `error`. |

## Available rules at a glance

| Rule id                                          | Ecosystem      | Category      | Autofix | Summary                                                                                                                             |
| ------------------------------------------------ | -------------- | ------------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `changesets/summary`                             | changesets     | correctness   | no      | Requires a changeset body to start with a summary heading.                                                                          |
| `changesets/no_section_headings`                 | changesets     | correctness   | no      | Rejects change-type headings inside changeset bodies.                                                                               |
| `changesets/prefer-inline`                       | changesets     | style         | yes     | Rewrites object change entries that repeat what the inline form already implies.                                                    |
| `changesets/bump/none`                           | changesets     | correctness   | no      | Applies scoped body policy to `none` bump entries.                                                                                  |
| `changesets/bump/patch`                          | changesets     | correctness   | no      | Applies scoped body policy to `patch` bump entries.                                                                                 |
| `changesets/bump/minor`                          | changesets     | correctness   | no      | Applies scoped body policy to `minor` bump entries.                                                                                 |
| `changesets/bump/major`                          | changesets     | correctness   | no      | Applies scoped body policy to `major` bump entries.                                                                                 |
| `changesets/types/<type>`                        | changesets     | correctness   | no      | Applies scoped body policy to a configured changelog type.                                                                          |
| `changesets/duplicate`                           | changesets     | correctness   | no      | Recognized compatibility switch for duplicate target validation; workspace validation rejects duplicate package entries regardless. |
| `cargo/dependency-field-order`                   | Cargo          | style         | yes     | Orders keys inside inline dependency tables.                                                                                        |
| `cargo/internal-dependency-workspace`            | Cargo          | correctness   | yes     | Requires internal crate dependencies to use `workspace = true`.                                                                     |
| `cargo/publishable-dependencies`                 | Cargo          | correctness   | no      | Prevents publishable crates from depending on unpublished workspace crates.                                                         |
| `cargo/required-package-fields`                  | Cargo          | correctness   | no      | Requires selected `[package]` metadata fields.                                                                                      |
| `cargo/sorted-dependencies`                      | Cargo          | style         | yes     | Sorts dependency tables alphabetically.                                                                                             |
| `cargo/unlisted-package-private`                 | Cargo          | correctness   | yes     | Requires unmanaged crates to set `publish = false`.                                                                                 |
| `cargo/manifest-repository`                      | Cargo          | correctness   | yes     | Requires `package.repository` to point at the root repository or the package subdirectory URL.                                      |
| `npm/workspace-protocol`                         | npm-family     | correctness   | yes     | Requires internal dependencies to use `workspace:` ranges.                                                                          |
| `npm/sorted-dependencies`                        | npm-family     | style         | yes     | Sorts dependency sections alphabetically.                                                                                           |
| `npm/required-package-fields`                    | npm-family     | correctness   | no      | Requires selected `package.json` metadata fields.                                                                                   |
| `npm/root-no-prod-deps`                          | npm-family     | best practice | yes     | Keeps production dependencies out of the workspace root package.                                                                    |
| `npm/no-duplicate-dependencies`                  | npm-family     | correctness   | yes     | Prevents the same dependency from appearing in multiple dependency sections.                                                        |
| `npm/unlisted-package-private`                   | npm-family     | correctness   | yes     | Requires unmanaged packages to set `private: true`.                                                                                 |
| `npm/manifest-repository`                        | npm-family     | correctness   | yes     | Requires `repository` in `package.json` to point at the root repository or the package subdirectory URL.                            |
| `dart/sdk-constraint-present`                    | Dart           | correctness   | no      | Requires `environment.sdk` in `pubspec.yaml`.                                                                                       |
| `dart/sdk-constraint-modern`                     | Dart           | best practice | no      | Enforces a modern SDK lower bound and, by default, an upper bound.                                                                  |
| `dart/dependency-sorted`                         | Dart           | style         | yes     | Sorts dependency sections in `pubspec.yaml`.                                                                                        |
| `dart/required-package-fields`                   | Dart           | correctness   | no      | Requires selected `pubspec.yaml` metadata fields.                                                                                   |
| `dart/no-git-dependencies-in-published-packages` | Dart           | correctness   | no      | Blocks `git:` dependencies in publishable packages unless allowed.                                                                  |
| `dart/unlisted-package-private`                  | Dart           | correctness   | yes     | Requires unmanaged packages to set `publish_to: none`.                                                                              |
| `dart/no-unexpected-dependency-overrides`        | Dart           | best practice | no      | Allows `dependency_overrides` only in approved packages.                                                                            |
| `dart/internal-path-dependency-policy`           | Dart           | best practice | no      | Enforces one policy for internal Dart dependency references.                                                                        |
| `dart/workspace-internal-version-consistency`    | Dart           | correctness   | no      | Requires internal hosted dependency ranges to match workspace package versions.                                                     |
| `dart/flutter-package-metadata-consistent`       | Dart / Flutter | correctness   | no      | Requires Flutter packages to declare the Flutter SDK dependency consistently.                                                       |
| `dart/assets-sorted`                             | Dart / Flutter | style         | yes     | Sorts Flutter assets and fonts.                                                                                                     |
| `dart/manifest-repository`                       | Dart           | correctness   | yes     | Requires `repository` in `pubspec.yaml` to point at the root repository or the package subdirectory URL.                            |

## Changeset lint rules

Changeset lint rules use the same `[lints.rules]` table as manifest rules. They are evaluated while markdown changesets are loaded by validation and release workflows.

```toml
[lints]
use = ["changesets/recommended"]

[lints.rules]
"changesets/no_section_headings" = "error"
"changesets/summary" = { level = "error", required = true, heading_level = 2, min_length = 12, max_length = 80, forbid_trailing_period = true, forbid_conventional_commit_prefix = true, require_description = true }
"changesets/bump/major" = { level = "error", required_sections = ["Impact", "Migration"], min_body_chars = 120, require_code_block = true }
"changesets/types/breaking" = { level = "error", forbidden_headings = ["Breaking", "Breaking changes"], required_sections = ["Impact", "Migration"], required_bump = "major" }
```

### `changesets/summary`

**Why:** every changeset should be understandable from a compact, release-note-ready heading.

**What it checks:** the first heading in a changeset body. It can require a heading, constrain its level and length, ban trailing periods, ban conventional-commit prefixes, and require descriptive body text after the heading.

**Useful options:**

- `required`: require the summary heading.
- `heading_level`: require a Markdown heading level from `1` to `6`.
- `min_length` / `max_length`: constrain summary text length.
- `forbid_trailing_period`: reject summaries ending in `.`.
- `forbid_conventional_commit_prefix`: reject summaries such as `feat: add parser`.
- `require_description`: require a non-empty paragraph after the heading.

### `changesets/no_section_headings`

**Why:** change types already come from the changeset entries. Repeating them as body headings creates noisy generated changelogs.

**With the rule:** headings that duplicate configured changelog types, such as `## Breaking` or `## Fix`, are rejected.

### `changesets/prefer-inline`

**Why:** change entries read best in the inline `target: type` form. Writing an object that only repeats what the inline form already implies adds noise for agents and humans authoring changesets.

**What it checks:** object (table) change entries whose fields are exactly equivalent to the inline form:

- `type` alone (`core: { type: "feat" }`),
- `type` plus a `bump` that the type already implies (`core: { type: "feat", bump: "minor" }`, since `feat` implies `minor`), and
- a bare `bump` whose keyword is also a configured change type that implies the same bump (`core: { bump: "minor" }`, since `minor` is a change type with default bump `minor`; the inline entry keeps the bump and gains the type).

Entries with a `version`, a `caused_by`, an unknown field, an unknown change type, or a `bump` that disagrees with the type default are left untouched, because the inline form cannot express them without changing meaning. A bare `bump: none` is also left alone: changeset validation rejects it outright.

**Before:**

```markdown
---
"@monochange/cli":
  bump: minor
  type: feat
---
```

**After (`monochange check --fix`):**

```markdown
---
"@monochange/cli": feat
---
```

The rule is on by default for every project and included in `changesets/recommended`.

### `changesets/bump/<severity>`

**Why:** different bump severities can require different explanation standards. A `major` bump often needs impact and migration notes, while a `patch` bump may only need a concise description.

**Supported severities:** `none`, `patch`, `minor`, and `major`.

**Useful options:**

- `required_sections`: headings that must appear in the body.
- `forbidden_headings`: headings that must not appear in the body.
- `min_body_chars` / `max_body_chars`: body length bounds.
- `require_code_block`: require a fenced code block.
- `required_bump`: require entries governed by this rule to use a specific bump severity.

### `changesets/types/<type>`

**Why:** changelog types can carry their own policy. For example, a `breaking` type can require migration notes even if a repository has multiple bump severities.

The `<type>` segment must match a configured changelog type. It accepts the same scoped options as `changesets/bump/<severity>`.

### `changesets/duplicate`

**Why:** a changeset should not target the same effective package more than once.

Duplicate package entries are rejected by workspace validation. The rule id remains recognized in `[lints.rules]` for compatibility with existing configurations that explicitly turn it on or off.

## Cargo manifest lint rules

Cargo rules apply to discovered `Cargo.toml` package manifests and, where needed, the workspace package graph.

### `cargo/dependency-field-order`

**Why:** keeps inline dependency tables visually consistent.

**What it checks:** preferred key order inside dependency tables:

1. `workspace` or `version`
2. `default-features` / `default_features`
3. `features`
4. other keys like `optional`, `path`, `registry`, `package`, `git`, `branch`, `tag`, `rev`

**Without the rule:**

```toml
serde = { features = ["derive"], workspace = true }
```

**With the rule:**

```toml
serde = { workspace = true, features = ["derive"] }
```

**Options:**

- `fix`: defaults to `true`; rewrites the dependency entry when safe.

### `cargo/internal-dependency-workspace`

**Why:** internal workspace dependencies should usually be declared through the workspace rather than carrying their own explicit version strings.

**Without the rule:**

```toml
[dependencies]
monochange_core = { path = "../monochange_core", version = "0.1.0" }
```

**With the rule:**

```toml
[dependencies]
monochange_core = { workspace = true }
```

**When to use it:** when the repository wants one workspace-owned version source for internal crates.

**Options:**

- `require_workspace`: defaults to `true`; require internal dependencies to use `workspace = true`.
- `fix`: defaults to `true`; rewrites safe internal dependency entries.

### `cargo/publishable-dependencies`

**Why:** a crate that can be published should not depend on an internal workspace crate that cannot be published. That leaves registry consumers unable to resolve the dependency.

**What it checks:** publishable Cargo packages and their internal Cargo dependencies. If the dependent package is publishable, any internal dependency it relies on must also be publishable.

**Without the rule:**

```toml
# crates/app/Cargo.toml
[package]
name = "app"
version = "0.1.0"

[dependencies]
internal_helper = { workspace = true }

# crates/internal_helper/Cargo.toml
[package]
name = "internal_helper"
version = "0.1.0"
publish = false
```

**With the rule:** either make `internal_helper` publishable, remove the dependency from the publishable crate, or mark the depending crate private too.

**Autofix:** no. This is a release policy decision, so monochange reports the dependency chain instead of changing publishability for you.

### `cargo/required-package-fields`

**Why:** published crates should consistently carry the metadata your repository expects.

**Default required fields:**

- `description`
- `license`
- `repository`

**Without the rule:**

```toml
[package]
name = "example"
version = "0.1.0"
```

**With the rule:** monochange reports the missing fields so package metadata stays consistent.

**Options:**

- `fields`: replace the default required-field list.

Example:

```toml
[lints.rules]
"cargo/required-package-fields" = { level = "error", fields = ["description", "license"] }
```

### `cargo/sorted-dependencies`

**Why:** alphabetized dependency tables are easier to review and reduce noisy diffs.

**Without the rule:**

```toml
[dependencies]
zzzz = "1.0"
aaaa = "1.0"
mmmm = "1.0"
```

**With the rule:**

```toml
[dependencies]
aaaa = "1.0"
mmmm = "1.0"
zzzz = "1.0"
```

**Options:**

- `fix`: defaults to `true`; rewrites dependency sections in sorted order.

### `cargo/unlisted-package-private`

**Why:** a Cargo package that is not listed in `monochange.toml` should not be accidentally publishable.

**With the rule:** monochange requires either:

- adding the package to `monochange.toml`, or
- marking it private with `publish = false`.

**Without the rule:**

```toml
[package]
name = "experimental-crate"
version = "0.1.0"
```

**With the rule:**

```toml
[package]
name = "experimental-crate"
version = "0.1.0"
publish = false
```

**Options:**

- `fix`: defaults to `true`; inserts `publish = false` when safe.

### `cargo/manifest-repository`

**Why:** package registry pages should send readers to the exact source directory for the package they are using. In monorepos, a root repository URL is correct for root-level packages, but packages under subdirectories should link to that subdirectory on the configured default branch.

**What it checks:** the rule compares `[package].repository` with the repository URL derived from `[source]` in `monochange.toml`:

- root-level packages must use the base repository URL, such as `https://github.com/acme/widgets`
- subdirectory packages must use `{repo_url}/tree/{default_branch}/{relative_package_dir}`, such as `https://github.com/acme/widgets/tree/main/crates/widget_core`
- if `[source]` is missing, the rule skips because monochange cannot derive the canonical repository URL

Cargo manifests may also use `repository = { workspace = true }`. By default, this rule resolves that inheritance from the root `Cargo.toml`'s `[workspace.package].repository`, falling back to root `[package].repository`. If the inherited root value does not point at the package subdirectory, the rule reports the package manifest and can replace the inherited inline table with an explicit repository URL.

**Without the rule:**

```toml
[package]
name = "widget_core"
version = "0.1.0"
repository = "https://github.com/acme/widgets"
```

**With the rule:**

```toml
[package]
name = "widget_core"
version = "0.1.0"
repository = "https://github.com/acme/widgets/tree/main/crates/widget_core"
```

**Configuration:**

```toml
[lints.rules]
"cargo/manifest-repository" = "error"
```

Set `allow_workspace_inheritance = true` only when you intentionally want to permit `repository = { workspace = true }` without resolving it against the package path:

```toml
[lints.rules]
"cargo/manifest-repository" = { level = "error", allow_workspace_inheritance = true }
```

**Autofix:** run `monochange check --fix` to insert a missing `repository`, replace an incorrect value, or convert `repository = { workspace = true }` into the explicit URL required for the package directory. There is no per-rule `fix` option; applying fixes is controlled by the CLI flag.

## npm-family manifest lint rules

npm-family rules apply to `package.json` manifests discovered through npm, pnpm, yarn, Bun, and Deno/npm-style package graphs.

### `npm/workspace-protocol`

**Why:** internal workspace dependencies should use the `workspace:` protocol so local workspace intent is explicit.

**Without the rule:**

```json
{
	"dependencies": {
		"@acme/shared": "^1.2.0"
	}
}
```

**With the rule:**

```json
{
	"dependencies": {
		"@acme/shared": "workspace:*"
	}
}
```

**When to use it:** npm, pnpm, yarn, and Bun workspaces where internal packages should not drift to plain registry ranges.

**Options:**

- `require_for_private`: defaults to `false`; also enforce the rule for private packages.
- `fix`: defaults to `true`; rewrites internal dependency ranges to `workspace:` ranges.

### `npm/sorted-dependencies`

**Why:** alphabetized dependency sections reduce review noise and make package diffs easier to scan.

**Without the rule:**

```json
{
	"dependencies": {
		"zod": "^4.0.0",
		"chalk": "^5.0.0"
	}
}
```

**With the rule:**

```json
{
	"dependencies": {
		"chalk": "^5.0.0",
		"zod": "^4.0.0"
	}
}
```

**Options:**

- `fix`: defaults to `true`; rewrites dependency sections in sorted order.

### `npm/required-package-fields`

**Why:** package metadata should stay consistent across publishable npm packages.

**Default required fields:**

- `description`
- `repository`
- `license`

**Without the rule:**

```json
{
	"name": "@acme/app",
	"version": "1.0.0"
}
```

**With the rule:** monochange reports the missing metadata fields.

**Options:**

- `fields`: replace the default required-field list.

### `npm/root-no-prod-deps`

**Why:** the workspace root `package.json` is usually orchestration-only and should keep runtime dependencies out of the root package.

**Without the rule:**

```json
{
	"dependencies": {
		"react": "^19.0.0"
	}
}
```

**With the rule:** move those to `devDependencies` when the root package is only a workspace manager.

**Options:**

- `fix`: defaults to `true`; moves root `dependencies` into `devDependencies`.

### `npm/no-duplicate-dependencies`

**Why:** the same dependency should not appear in multiple dependency sections unless the repository has a very deliberate reason.

**Without the rule:**

```json
{
	"dependencies": {
		"typescript": "^5.0.0"
	},
	"devDependencies": {
		"typescript": "^5.0.0"
	}
}
```

**With the rule:** monochange reports the duplicate and can remove redundant entries from later sections when safe.

**Options:**

- `fix`: defaults to `true`; removes duplicate entries from later sections.

### `npm/unlisted-package-private`

**Why:** a package not declared in `monochange.toml` should not remain publishable by accident.

**With the rule:** monochange requires either:

- adding the package to `monochange.toml`, or
- marking it private in `package.json`.

**Without the rule:**

```json
{
	"name": "@acme/experimental",
	"version": "0.1.0"
}
```

**With the rule:**

```json
{
	"name": "@acme/experimental",
	"private": true,
	"version": "0.1.0"
}
```

**Options:**

- `fix`: defaults to `true`; inserts `private: true` when safe.

### `npm/manifest-repository`

**Why:** npm package metadata should link users to the exact source folder for that package. In monorepos, a root repository URL is correct only for root-level packages; packages in subdirectories should link directly to their package directory on the configured default branch.

**What it checks:** the rule compares `repository` in `package.json` with the repository URL derived from `[source]` in `monochange.toml`:

- root-level packages must use the base repository URL, such as `https://github.com/acme/widgets`
- subdirectory packages must use `{repo_url}/tree/{default_branch}/{relative_package_dir}`, such as `https://github.com/acme/widgets/tree/main/packages/widget-core`
- if `[source]` is missing, the rule skips because monochange cannot derive the canonical repository URL

**Without the rule:**

```json
{
	"name": "@acme/widget-core",
	"version": "0.1.0",
	"repository": "https://github.com/acme/widgets"
}
```

**With the rule:**

```json
{
	"name": "@acme/widget-core",
	"version": "0.1.0",
	"repository": "https://github.com/acme/widgets/tree/main/packages/widget-core"
}
```

**Configuration:**

```toml
[lints.rules]
"npm/manifest-repository" = "error"
```

**Autofix:** run `monochange check --fix` to insert a missing `repository` or replace an incorrect value. There is no per-rule `fix` option; applying fixes is controlled by the CLI flag.

## Dart manifest lint rules

Dart rules apply to `pubspec.yaml` manifests, including Flutter packages when a pubspec has Flutter-specific metadata.

### `dart/sdk-constraint-present`

**Why:** every managed Dart package should declare the SDK range it expects rather than inheriting whatever the developer machine happens to provide.

**With the rule:** monochange reports any `pubspec.yaml` that omits `environment.sdk`.

**Without the rule:**

```yaml
name: app
version: 1.0.0
```

**With the rule:**

```yaml
name: app
version: 1.0.0
environment:
  sdk: ">=3.6.0 <4.0.0"
```

### `dart/sdk-constraint-modern`

**Why:** old or overly broad SDK ranges quietly expand your support policy and make releases harder to reason about.

**Default policy:**

- minimum lower bound: `3.0.0`
- upper bound required by default

**Options:**

- `minimum`: override the minimum lower bound for your workspace.
- `require_upper_bound`: set to `false` if your policy intentionally omits an upper bound.

Example:

```toml
[lints.rules]
"dart/sdk-constraint-modern" = { level = "warning", minimum = "3.6.0", require_upper_bound = false }
```

### `dart/dependency-sorted`

**Why:** alphabetized `dependencies`, `dev_dependencies`, and `dependency_overrides` blocks reduce review noise and make Dart manifest diffs easier to scan.

**Without the rule:**

```yaml
dependencies:
  zeta: ^1.0.0
  alpha: ^1.0.0
```

**With the rule:**

```yaml
dependencies:
  alpha: ^1.0.0
  zeta: ^1.0.0
```

**Options:**

- `fix`: defaults to `true`; rewrites dependency sections in sorted order.

### `dart/required-package-fields`

**Why:** managed publishable Dart packages should carry the metadata your repository expects before release.

**Default required fields:**

- `description`
- `repository`
- `license`

**Without the rule:**

```yaml
name: app
version: 1.0.0
```

**With the rule:** monochange reports missing metadata fields for publishable packages.

**Options:**

- `fields`: replace the default required-field list.

Example:

```toml
[lints.rules]
"dart/required-package-fields" = { level = "error", fields = ["description", "repository"] }
```

### `dart/no-git-dependencies-in-published-packages`

**Why:** published Dart packages should resolve from hosted dependencies, not source-control dependencies, unless the repository explicitly allows an exception.

**Without the rule:**

```yaml
dependencies:
  shared:
    git:
      url: https://github.com/acme/shared.git
```

**With the rule:** monochange reports `git:` dependencies in publishable packages unless the dependency name appears in the allow list.

**Options:**

- `allow`: list dependency names that may use `git:` sources.

Example:

```toml
[lints.rules]
"dart/no-git-dependencies-in-published-packages" = { level = "error", allow = ["shared"] }
```

### `dart/unlisted-package-private`

**Why:** a Dart package that is not listed in `monochange.toml` should not be accidentally publishable.

**With the rule:** monochange requires either:

- adding the package to `monochange.toml`, or
- marking it private with `publish_to: none`.

**Without the rule:**

```yaml
name: experimental
version: 0.1.0
```

**With the rule:**

```yaml
name: experimental
version: 0.1.0
publish_to: none
```

**Options:**

- `fix`: defaults to `true`; inserts `publish_to: none` when safe.

### `dart/no-unexpected-dependency-overrides`

**Why:** `dependency_overrides` are sometimes necessary, but they should usually be limited to private packages or a small allow list of explicitly approved packages.

**With the rule:** monochange reports `dependency_overrides` unless they are allowed by privacy or package name.

**Options:**

- `allow_for_private`: defaults to `true`; allow overrides in private packages.
- `allow_packages`: list package names that may keep `dependency_overrides`.

Example:

```toml
[lints.rules]
"dart/no-unexpected-dependency-overrides" = { level = "warning", allow_for_private = true, allow_packages = ["app_shell"] }
```

### `dart/internal-path-dependency-policy`

**Why:** monorepos usually want one consistent policy for how internal Dart packages reference each other.

**Default policy:** strict mode expects internal packages to use `path:` references unless the pubspec declares `resolution: workspace`.

With Dart workspace resolution, Dart resolves versioned internal dependencies to local workspace packages automatically. In that mode, monochange requires version constraints and reports `path:` references with the message "use version constraints (not `path:`) when resolution is workspace".

**Options:**

- `mode`: choose `"path"` or `"hosted"` for packages that do not use `resolution: workspace`.

Example:

```toml
[lints.rules]
"dart/internal-path-dependency-policy" = { level = "error", mode = "hosted" }
```

### `dart/workspace-internal-version-consistency`

**Why:** when workspace packages reference each other with hosted version ranges, those ranges should not drift away from the current workspace version.

**With the rule:** monochange compares internal dependency version references against the discovered workspace package version and reports mismatches. Use `monochange versions --dry-run` to preview automatic repairs for supported manifests, then rerun without `--dry-run` to update supported internal dependency references.

### `dart/flutter-package-metadata-consistent`

**Why:** packages with a `flutter` section should declare the Flutter SDK dependency consistently so they are unmistakably Flutter packages.

**With the rule:** monochange requires `dependencies.flutter = { sdk = flutter }` in `pubspec.yaml` terms, expressed as the YAML mapping form.

**Without the rule:**

```yaml
name: widgets
flutter:
  assets:
    - assets/logo.png
```

**With the rule:**

```yaml
name: widgets
dependencies:
  flutter:
    sdk: flutter
flutter:
  assets:
    - assets/logo.png
```

### `dart/assets-sorted`

**Why:** stable ordering for `flutter.assets` and `flutter.fonts` reduces noisy diffs in Flutter packages.

**Without the rule:**

```yaml
flutter:
  assets:
    - assets/zeta.png
    - assets/alpha.png
```

**With the rule:**

```yaml
flutter:
  assets:
    - assets/alpha.png
    - assets/zeta.png
```

**Options:**

- `fix`: defaults to `true`; rewrites Flutter assets and fonts in sorted order.

### `dart/manifest-repository`

**Why:** Dart and Flutter package metadata should link users to the exact source folder for that package. In monorepos, a root repository URL is correct only for root-level packages; packages in subdirectories should link directly to their package directory on the configured default branch.

**What it checks:** the rule compares `repository` in `pubspec.yaml` with the repository URL derived from `[source]` in `monochange.toml`:

- root-level packages must use the base repository URL, such as `https://github.com/acme/widgets`
- subdirectory packages must use `{repo_url}/tree/{default_branch}/{relative_package_dir}`, such as `https://github.com/acme/widgets/tree/main/packages/widget_core`
- if `[source]` is missing, the rule skips because monochange cannot derive the canonical repository URL

**Without the rule:**

```yaml
name: widget_core
version: 0.1.0
repository: https://github.com/acme/widgets
```

**With the rule:**

```yaml
name: widget_core
version: 0.1.0
repository: https://github.com/acme/widgets/tree/main/packages/widget_core
```

**Configuration:**

```toml
[lints.rules]
"dart/manifest-repository" = "error"
```

**Autofix:** run `monochange check --fix` to insert a missing `repository` or replace an incorrect value. There is no per-rule `fix` option; applying fixes is controlled by the CLI flag.

## What `monochange check` looks like in practice

Use plain text for local review:

```bash
monochange check
```

Apply safe auto-fixes where possible:

```bash
monochange check --fix
```

Use JSON for CI or MCP-style tooling:

```bash
monochange check --format json
```

`monochange check` fails when lint errors are present, so it is appropriate for CI gates.

## Recommended workflow

For repository work:

```bash
monochange step validate
monochange check
monochange step prepare-release --dry-run --diff
```

If you changed shared docs too:

```bash
devenv shell docs:check
```

<!-- {/lintingPolicyReference} -->

<!-- {@manifestRepositoryLintReadmeSummary} -->

### Optional repository URL lint rules

monochange includes opt-in repository URL lint rules for Cargo, Dart, and npm-family manifests:

```toml
[lints.rules]
"cargo/manifest-repository" = "error"
"dart/manifest-repository" = "error"
"npm/manifest-repository" = "error"
```

These rules compare each manifest's `repository` field with the repository configured under `[source]` in `monochange.toml`. Root-level packages use the base repository URL, while packages in subdirectories use `{repo_url}/tree/{default_branch}/{relative_package_dir}`. Run `monochange check --fix` to insert or update repository fields; there is no per-rule `fix` option.

Cargo also resolves `repository = { workspace = true }` by reading the root manifest's `[workspace.package].repository` (falling back to root `[package].repository`). If you intentionally want to allow workspace inheritance without validating the package-specific URL, configure:

```toml
[lints.rules]
"cargo/manifest-repository" = { level = "error", allow_workspace_inheritance = true }
```

For full rule-by-rule behavior, see the manifest linting reference and `monochange lint explain <rule-id>`.

<!-- {/manifestRepositoryLintReadmeSummary} -->
