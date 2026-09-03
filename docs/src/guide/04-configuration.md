# Configuration

Repository configuration lives in `monochange.toml`.

## JSON Schema

A JSON Schema for editor support is published with the book at <https://monochange.github.io/monochange/schemas/monochange.schema.json>. That URL is the moving "current" alias for the latest docs. Stable generated copies use public schema-version suffixes, starting with <https://monochange.github.io/monochange/schemas/monochange.v0.1.schema.json>.

Schema-aware TOML editors such as Taplo can opt in with a comment directive at the top of `monochange.toml`:

```text
#:schema https://monochange.github.io/monochange/schemas/monochange.schema.json
```

The same file is also available from GitHub raw content at <https://raw.githubusercontent.com/monochange/monochange/main/docs/src/schemas/monochange.schema.json>. Regenerate committed schema assets with `schema:update` and verify them with `schema:check`; `lint:all` runs the check in CI.

## Shared documentation

This book is maintained with `mdt` so shared content blocks stay synchronized across pages.

- Shared blocks live in `.templates/*.t.md`
- Consumer files include them with `<!-- {=templateName} -->` directives
- Run `mdt update` (or `docs:update` in this repository) after changing any template or consumer block
- Run `mdt check` (or `docs:check`) before opening a PR to verify synchronization

When you edit a template such as `.templates/cli-steps.t.md`, the changes propagate to every documentation file that references it. This keeps the book, readmes, and inline help consistent without manual copying.

## Defaults

<!-- {=configurationDefaultsSnippet} -->

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

## Packages

Declare every release-managed package explicitly.

<!-- {=configurationVersionGroupsSnippet} -->

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

## Bump propagation to dependents

<!-- {=configurationBumpPropagationSnippet} -->

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

Required fields:

- `path`
- `type`, unless `[defaults].package_type` is set

Supported `type` values:

- `cargo`
- `npm`
- `deno`
- `dart`
- `flutter`
- `python`

Optional package fields:

- `type`, when `[defaults].package_type` is set
- `changelog`
- `empty_update_message`
- `publish`
- `versioned_files`
- `tag`
- `release`
- `version_format`

`version_format` controls the Git tag identity used for package and group releases. It defaults to `namespaced` when no configuration is set, which produces collision-safe tags like `my-package/v1.2.3`. Set it to `primary` for the single top-level release identity that should use tags like `v1.2.3`. You can also provide a custom tag template with `{{ name }}`, `{{ version }}`, and `{{ ecosystem }}`:

```toml
[package.cli]
path = "crates/cli"
type = "cargo"
version_format = "{{ ecosystem }}/{{ name }}/v{{ version }}"
```

Custom formats must include `{{ version }}`, render to valid Git tag names without whitespace or other invalid ref characters, and must not collide with another release owner for the same sample version. If several packages share a custom format, include `{{ name }}` so the generated tags remain unique.

`changelog` accepts three forms on packages:

- `true` → use `{{ path }}/CHANGELOG.md`
- `false` → disable the package changelog
- `"some/path.md"` → use that exact path

`[defaults].changelog` also accepts three forms:

- `true` → default every package to `{{ path }}/CHANGELOG.md`
- `false` → default every package to no changelog
- `"{{ path }}/changelog.md"` or another pattern → replace `{path}` with each package path

A package-level `changelog` value overrides the default for that package.

The table form also accepts `initial_header`. monochange renders this Markdown only when a changelog file is created from empty content. Existing changelog preambles are preserved and are not rewritten on later releases. If `initial_header` is omitted or blank, monochange uses the selected format's built-in header: `keep_a_changelog` gets the Keep a Changelog/SemVer preamble, and `monochange` gets the monochange-managed preamble. Package and group changelog tables can override the default header.

```toml
[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"
initial_header = """
# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).
"""
```

`initial_header` templates can use release context such as `{{ monochange_version }}`, `{{ config_path }}`, `{{ monochange_config_path }}`, `{{ workspace_root }}`, `{{ changelog_path }}`, `{{ changelog_format }}`, `{{ package }}`, `{{ package_name }}`, `{{ package_id }}`, `{{ package_path }}`, `{{ group }}`, `{{ group_name }}`, `{{ group_id }}`, `{{ member_count }}`, `{{ members }}`, `{{ release_owner }}`, `{{ release_owner_kind }}`, `{{ version }}`, `{{ new_version }}`, and `{{ current_version }}`.

`empty_update_message` lets changelog targets render a readable fallback entry when a version update is required but no direct release notes were recorded for that target. This is especially useful for grouped packages that keep their own changelog entries even when only another member of the group changed.

`empty_update_message` can be set on:

- `[defaults]`
- `[package.<id>]`
- `[group.<id>]`

`extra_changelog_sections` can also be set on:

- `[defaults]`
- `[package.<id>]`
- `[group.<id>]`

Defaults are inherited by packages and groups; package/group definitions append target-specific sections on top of the workspace defaults.

Template placeholders may include:

- `{{ package }}` / `{{ package_name }}`
- `{{ package_id }}`
- `{{ group }}` / `{{ group_name }}`
- `{{ group_id }}`
- `{{ version }}` / `{{ new_version }}`
- `{{ current_version }}` / `{{ previous_version }}`
- `{{ bump }}`
- `{{ trigger }}`
- `{{ ecosystem }}`
- `{{ release_owner }}` / `{{ release_owner_kind }}`
- `{{ members }}` / `{{ member_count }}` for group changelogs
- `{{ reasons }}`

Fallback order:

- package changelog entries: package → group → defaults → built-in message
- group changelog entries: group → defaults → built-in message

The built-in grouped-package fallback reads:

> No package-specific changes were recorded; `{{ package }}` was updated to {{ version }} as part of group `{{ group }}`.

## Prerelease mode

Enable `[prerelease]` when you want repeatable alpha/rc/binary prerelease builds before the final stable release. Prerelease mode writes SemVer prerelease versions into manifests by default, keeps changesets for the later stable release, skips changelog file updates by default, and does not publish packages unless explicitly enabled.

```toml
[prerelease]
enabled = true
channel = "alpha"
numbering = "increment" # increment | date | datetime
base = "planned" # planned | current-stable | fixed
base_version = "0.0.0" # required when base = "fixed"
branches = [
	"next",
	"prerelease/*",
] # optional; overrides [source.releases].branches for tag/publish checks while enabled

write_manifests = true
keep_changesets = true
changelog = false
release_notes = true
publish_packages = false
```

Base strategies:

- `planned`: compute the next stable version from changesets and dependency propagation, then append the prerelease suffix.
- `current-stable`: use the current/original stable manifest version as the prerelease base.
- `fixed`: use `base_version`, which is useful for binary/nightly workflows such as `0.0.0-alpha.0`.

When no changesets exist, prerelease mode still synthesizes release decisions from discovered packages and version groups. Repeated prerelease runs persist state in `.monochange/prerelease-state.json` so a series advances from `alpha.0` to `alpha.1` without repeatedly reapplying the same stable bump. Set `branches` when prerelease tag/publish workflow steps should be allowed from a different branch set than stable releases. Disable prerelease mode for the final stable release; successful stable preparation removes the state file. If prerelease mode is disabled and `.monochange/prerelease-state.json` is still present, validation/check fails so stale prerelease state is not ignored.

## Package publishing

Built-in package publishing is configured through `publish` on packages and ecosystems.

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true

[ecosystems.npm.publish_order]
dependency_fields = ["dependencies", "devDependencies", "peerDependencies", "catalogDependencies"]

[ecosystems.npm.publish.trusted_publishing]
workflow = "publish.yml"
environment = "publisher"

[ecosystems.npm.publish.attestations]
require_registry_provenance = true

[package.web.publish]
mode = "builtin"

[package.web.publish.placeholder]
readme_file = "docs/web-placeholder.md"

[package.legacy.publish]
trusted_publishing = false
```

Supported fields:

- `enabled` - include this package in managed publishing
- `mode` - `builtin` or `external`. When `builtin` (the default), monochange's built-in publisher handles release publishing. When `external`, monochange skips the package during release publishing (`PublishPackages`): your own CI or scripts handle release publishing instead. The `mode` setting does **not** affect placeholder publishing (`PlaceholderPublish`), which processes all packages with `publish.enabled = true`.
- `registry` - public registry override for the package ecosystem
- `trusted_publishing` - `true`/`false` or a table with `enabled`, `repository`, `workflow`, and `environment`
- `attestations.require_registry_provenance` - require registry-native package provenance when the selected registry/provider capability supports it
- `rate_limits.enforce` - block built-in publish runs when the selected package set exceeds a known single registry window
- `fail_on_duplicate` - fail the publish step when a version is already published on the registry instead of skipping it (default: `false`); the built-in `publish-packages` step exposes the same policy as the `--fail-on-duplicate` CLI input for a single run
- `timeout.timeout_seconds` - maximum seconds a single package publish command may run before it is killed and retried; set to `0` to disable the timeout (default: `60`)
- `timeout.retries` - number of times to retry a publish command that times out before reporting the package as failed (default: `2`)
- `placeholder.readme` - inline placeholder README content
- `publish_order.dependency_fields` - ecosystem-level dependency fields used to topologically order package publishes
- `placeholder.readme_file` - workspace-relative file to use as placeholder README content

Inheritance flows from `[ecosystems.<name>.publish]` to matching packages, and package-level values override the inherited ecosystem defaults. Configure shared trusted-publishing, attestation, and context policy on the ecosystem, then use package-level publish settings for opt-outs or package-specific workflows.

Built-in publishing targets only the canonical public registry for each supported ecosystem:

- Cargo → `crates.io`
- npm packages → `npm`
- Deno packages → `jsr`
- Dart / Flutter packages → `pub.dev`
- Python packages → `pypi`
- Go modules → `go_proxy` via VCS tags

Private registries and custom publication flows are still external. For those packages, set `mode = "external"` and handle release publication outside monochange. Placeholder publishing (`monochange step placeholder-publish`) still works for external-mode packages because it is a bootstrap utility, not a release publishing step.

### Placeholder publishing

`monochange step placeholder-publish` exists for the bootstrap case where a package must already exist in the registry before you can finish automation setup such as trusted publishing.

Placeholder publishing works for all packages with `publish.enabled = true`, including those set to `publish.mode = "external"`. The `mode` field controls who handles **release** publishing (monochange's built-in publisher vs your own CI/scripts); it does not affect placeholder publishing because that is a one-time bootstrap utility, not a release step. To opt out of placeholder publishing entirely, set `publish.enabled = false`.

For each publishable package, monochange:

- checks whether the package already exists in its configured public registry
- skips packages that already exist
- publishes a placeholder package only for packages that are missing
- uses version `0.0.0`
- renders a default placeholder README unless `placeholder.readme` or `placeholder.readme_file` overrides it

`placeholder.readme` and `placeholder.readme_file` are mutually exclusive. If both are set, config validation fails.

### Publish order dependency fields

`publish_order.dependency_fields` controls which manifest dependency fields create publish-order edges for an ecosystem. npm defaults to `dependencies` and `devDependencies`, so peer packages do not block publishing unless opted in. Cargo defaults stay `dependencies`, `dev-dependencies`, and `build-dependencies`. Deno defaults to `dependencies` and `imports`, Dart/Flutter default to `dependencies` and `dev_dependencies`, Python defaults to `dependencies`, and Go defaults to `require`. Optional Python extras (`optional-dependencies`) and Poetry groups (`group.dependencies`) only affect publish order when you opt in.

```toml
[ecosystems.npm.publish_order]
# Add peer and custom package.json fields.
dependency_fields = ["dependencies", "devDependencies", "peerDependencies", "catalogDependencies"]
```

```toml
[ecosystems.npm.publish_order]
# Or remove devDependencies from publish ordering.
dependency_fields = ["dependencies"]
```

```toml
[ecosystems.python.publish_order]
# Include optional dependency groups in Python publish ordering.
dependency_fields = ["dependencies", "optional-dependencies", "group.dependencies"]
```

```toml
[ecosystems.go.publish_order]
# An empty list disables Go require-based publish ordering.
dependency_fields = []
```

The same resolved policy is used by `monochange step plan-publish-rate-limits` and `monochange step publish-packages`.

### Trusted publishing

`trusted_publishing` lets you tell monochange that package publication is expected to come from a verified GitHub Actions context.

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.trusted_publishing]
repository = "owner/repo"
workflow = "publish.yml"
environment = "publisher"

[package.cli.publish.trusted_publishing]
workflow = "publish-cli.yml"

[package.legacy.publish]
trusted_publishing = false
```

When `trusted_publishing` is enabled:

- npm package publishing must run from a verifiable CI/OIDC identity and must not use long-lived npm token environment variables
- npm trusted-publisher enrollment is manual or external: monochange can render the expected `npm trust github ...` repair command and verify the GitHub workflow context, but `monochange step publish-packages` does not run `npm trust` automatically
- trusted npm publishing uses the `npm` CLI directly; pnpm workspaces still use pnpm for non-trusted npm publishing paths
- Cargo, `jsr`, `pub.dev`, and `PyPI` also require manual trusted-publishing setup; monochange reports the setup URL and blocks built-in release publishing until trust is configured

### Attestation policy

`publish.attestations.require_registry_provenance` is separate from `publish.trusted_publishing`. Trusted publishing must be enabled first, then the attestation policy tells monochange to require registry-native package provenance where the selected registry and CI provider support it.

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.attestations]
require_registry_provenance = true

[package.legacy.publish.attestations]
require_registry_provenance = false
```

monochange treats npm provenance and JSR package provenance as enforceable built-in registry provenance. `PyPI` PEP 740 attestations are modeled in the capability matrix, but `require_registry_provenance` is rejected for `PyPI` until the built-in Python publisher exposes a publish command that can require uploading those attestations. `crates.io`, `pub.dev`, Go proxy publishing, and custom registries are also rejected when this requirement is enabled because monochange cannot verify equivalent registry-native package attestations for those flows.

GitHub release asset attestations are a separate release policy under `[source.releases.attestations]` and are valid only for the GitHub source provider:

```toml
[source.releases.attestations]
require_github_artifact_attestations = true
```

For a GitHub-focused setup guide with exact registry fields, commands, and workflow requirements, see [Trusted publishing and OIDC](./07-trusted-publishing.md). For monorepo workflow and tag-shape recommendations, see [Multi-package publishing patterns](./14-multi-package-publishing.md).

monochange resolves the GitHub trust context from:

- explicit `repository`, `workflow`, and `environment` values in config
- otherwise `[source]` plus GitHub Actions environment such as `GITHUB_WORKFLOW_REF` and `GITHUB_JOB`
- and, when possible, the workflow job environment declared in `.github/workflows/<file>.yml`

If monochange cannot determine the GitHub repository or workflow for an npm package, it cannot render a precise `npm trust github ...` repair command or verify the expected GitHub context.

### Implementation limits

The built-in package publishing flow is intentionally narrow:

- no private or custom registry support in `mode = "builtin"`
- rate-limit planning can batch work and enforce single-window safety, but monochange still does not sleep across windows or requeue later batches automatically
- registry-side trusted-publisher enrollment is still manual for every registry; npm is special only because monochange can render and verify the GitHub setup context

If your workflow needs any of these, keep the package on `mode = "external"` and let your own CI or scripts own publication.

For end-to-end GitHub and GitLab examples - including npm trusted publishing on GitHub and token/external-mode patterns on GitLab - see [Advanced: CI, package publishing, and release PR flows](./13-ci-and-publishing.md).

## Groups

Groups own outward release identity for their member packages.

```toml
[group.sdk]
packages = ["sdk-core", "web-sdk", "mobile-sdk"]
changelog = "changelog.md"
versioned_files = [{ path = "group.toml", type = "cargo" }]
tag = true
release = true
version_format = "primary"
```

Rules:

- group members must already be declared under `[package.<id>]`
- package and group ids share one namespace
- a package may belong to only one group
- only one package or group may use `version_format = "primary"`
- custom `version_format` templates must include `{{ version }}` and render unique, valid Git tag names; include `{{ name }}` when sharing a template across release owners
- group `tag`, `release`, and `version_format` override member package release identity
- package changelogs and package `versioned_files` still apply when grouped
- grouped packages can customize fallback changelog entries with `empty_update_message` when no direct package notes are present
- `[group.<id>.changelog].include` can filter which member-targeted changesets appear in the group changelog without changing release planning or package changelogs

For grouped changelog filtering, use the changelog table form:

```toml
[group.sdk.changelog]
path = "docs/sdk-changelog.md"
include = ["sdk-cli"]
```

`include` accepts:

- `"all"` - include direct group-targeted changesets and all member-targeted changesets (default)
- `"group-only"` - include only direct group-targeted changesets
- `[]` or `["package-id", ...]` - include direct group-targeted changesets plus member-targeted changesets only when every target in that group is listed

## Versioned files

`versioned_files` are additional managed files beyond native manifests.

Examples:

```toml
# package-scoped shorthand infers the package ecosystem
versioned_files = ["Cargo.toml"]
versioned_files = ["**/crates/*/Cargo.toml"]

# explicit typed entries remain available
versioned_files = [{ path = "group.toml", type = "cargo", name = "sdk-core" }]
versioned_files = [{ path = "docs/version.txt", type = "cargo" }]
versioned_files = [
	{ path = "Cargo.toml", type = "cargo", fields = ["workspace.metadata.bin.monochange.version"], prefix = "" },
]
versioned_files = [
	{ path = "package.json", type = "npm", fields = ["metadata.bin.monochange.version"] },
]

# generic format entries update explicit fields in non-ecosystem files
versioned_files = [
	{ path = "metadata.json", format = "json", fields = ["release.version"] },
	{ path = "tools.toml", format = "toml", fields = ["tool.sdk.version"] },
	{ path = "pubspec-overrides.yaml", format = "yaml", fields = ["metadata.sdkVersion"] },
	{ path = ".env", format = "env", fields = ["VERSION"] },
]

# ecosystem-level defaults inherited by matching packages
[ecosystems.npm]
versioned_files = ["**/packages/*/package.json"]
```

Typed manifest entries can update dependency sections and arbitrary string fields inside TOML or JSON manifests. Dependency targets in `versioned_files` must reference declared package ids. Groups must use explicit typed entries because monochange cannot infer a group ecosystem from a bare string.

### Format versioned files

Use `format` when a version lives in a structured or key/value file that should not receive ecosystem-specific dependency handling. Supported values are `json`, `toml`, `yaml`, `yml`, and `env`.

```toml
[package.core]
path = "crates/core"
versioned_files = [
	{ path = "metadata.json", format = "json", fields = ["release.version"] },
	{ path = ".env", format = "env", fields = ["VERSION"] },
]
```

Key rules:

- `format` entries cannot set `type` or `regex`
- `fields` is required and must name every value to update; monochange does not infer ecosystem defaults in format mode
- JSON, TOML, YAML, and YML fields use dot-separated object/table paths such as `release.version`
- env fields use exact keys such as `VERSION` and update existing `KEY=value` or `export KEY=value` lines
- field names can include `{{ name }}` and `{{ version }}` placeholders for simple context-aware paths or keys

### Regex versioned files

<!-- {=configurationRegexVersionedFilesSnippet} -->

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

## Lockfile commands

By default monochange rewrites supported lockfiles directly from the release plan. That keeps normal `monochange run release` runs close to `--dry-run` speed instead of launching package managers just to rewrite workspace version strings.

Built-in direct lockfile updates cover:

- Cargo: `Cargo.lock`
- npm-family: `package-lock.json`, `pnpm-lock.yaml`, `bun.lock`, and `bun.lockb`
- Deno: `deno.lock`
- Dart / Flutter: `pubspec.lock`

For Python projects, monochange infers package-manager lockfile commands instead of mutating lockfiles directly: `uv.lock` uses `uv lock`, and `poetry.lock` uses `poetry lock --no-update`. Unknown Python lockfile names are skipped rather than guessed.

If you configure `lockfile_commands` for an ecosystem, monochange stops using the built-in direct updater for that ecosystem and those commands fully own lockfile refresh. Use that escape hatch only when your workspace needs package-manager-side regeneration beyond version rewrites.

For Cargo specifically, monochange no longer falls back to `cargo generate-lockfile` automatically when a lockfile looks incomplete. That keeps `monochange run release` on the fast path and leaves the final dependency-resolution refresh under your control: either configure `[ecosystems.cargo].lockfile_commands` explicitly or run `cargo generate-lockfile` / `cargo check` yourself afterwards.

If you want to measure that tradeoff before opting into a refresh command, run the `prepare_release_apply_cargo_lockfile_refresh` Criterion benchmark. It compares the default `direct_rewrite` path against an explicit `full_refresh_command` run on the same synthetic Cargo workspace.

```toml
[ecosystems.npm]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
	{ command = "npm install --package-lock-only", cwd = "packages/legacy", shell = true },
]
```

`cwd` is resolved relative to the workspace root. `shell = false` runs the command directly, `shell = true` uses `sh -c`, and `shell = "bash"` uses a custom shell binary.

## CLI commands

CLI workflow commands are user-defined commands that run as `monochange run <command>`. Each `[cli.<command>]` table in `monochange.toml` defines one workflow with its own help text, inputs, and ordered step list.

`monochange init` writes a minimal starter config and does not seed default `[cli.*]` workflow aliases. Add `[cli.<command>]` tables only for repository-specific workflows that need to chain multiple steps, expose custom names, or run shell `Command` steps.

Built-in steps are also available directly as immutable `monochange step <name>` commands. The binary generates those commands from the step schemas, so `monochange step discover`, `monochange step prepare-release`, `monochange step affected-packages`, and the other step commands do not require config entries. Use `monochange step <name>` in CI when you want a stable built-in operation without depending on a repository-defined wrapper.

Some top-level names are reserved for binary commands, including `init`, `mcp`, `help`, `version`, `analyze`, `check`, and `step`. The `step` command namespace is reserved for immutable built-in step commands, and `run` is reserved for executing configured workflows. Do not define `[cli.step]` or `[cli.run]` tables.

<!-- {=cliStepExplicitInputInheritance} -->

## Explicit step input inheritance

Config-defined workflow commands have two input layers:

1. `[[cli.<command>.inputs]]` declares the flags and arguments accepted by `monochange run <command>`.
2. `inputs` on each step decides which of those parsed command inputs are visible while that step runs.

Command inputs are **not inherited automatically**. A step receives a command input only when the step explicitly lists it. This makes wrappers predictable when a command-level flag and a step-specific input share the same name.

Use the array shorthand when a step should inherit command inputs unchanged:

```toml
[cli.discover]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "json-min"], default = "text" },
]
steps = [
	{ type = "Discover", inputs = ["format"] },
]
```

Use the map form when a step needs fixed values, renamed values, templates, or a mixture of inherited and overridden values:

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "json-min", "markdown"], default = "markdown" },
	{ name = "open_as_draft", type = "boolean", default = false },
]
steps = [
	{ type = "PrepareRelease", inputs = ["format"] },
	{ type = "OpenReleaseRequest", inputs = { format = "markdown", draft = "{{ inputs.open_as_draft }}" } },
]
```

Step-local `when` expressions and command templates evaluate against the same explicit step input context. If a `when` condition references `inputs.publish`, the step must include `publish` in its `inputs` array or map. Use `inputs = ["publish"]` for unchanged inheritance, or `inputs = { publish = "{{ inputs.publish }}" }` when you need the map form for other overrides.

Override values in the map form accept native TOML literals: strings, booleans, integers, and floats. Booleans stay booleans in the parsed model and are stringified to `"true"`/`"false"` when the step runs; numbers are coerced to their string form at parse time, so writing `{ jobs = 4, ratio = 2.5 }` is exactly the same as writing `{ jobs = "4", ratio = "2.5" }`:

```toml
[[cli.release.steps]]
name = "publish"
type = "Command"
command = "npm publish --jobs {{ inputs.jobs }}"
inputs = { jobs = 4, dry_run = true }
```

### Interactive command steps

Add an explicit `interactive` boolean input to the command, pass it to a `Command` step, and run the workflow with `--interactive` when you want the command to own the terminal. The step inherits stdio for that run, so prompts and terminal UIs work, and the progress spinner is suppressed while the command runs.

```toml
[cli.publish]
help_text = "Publish packages"

[[cli.publish.inputs]]
name = "interactive"
type = "boolean"
default = false

[[cli.publish.steps]]
name = "publish"
type = "Command"
command = "npm publish"
inputs = ["interactive"]
```

```bash
monochange run publish --interactive
```

Leave `interactive` unset (the default) for CI and scripted runs so those commands stay non-interactive. Interactive steps do not capture output: `steps.<id>.stdout` and `steps.<id>.stderr` are empty for them, so downstream steps cannot read what an interactive command printed.

Built-in `monochange step <name>` commands are different: they are generated directly from the step schema, so their CLI flags map to that single step without a `[cli.*]` wrapper.

<!-- {/cliStepExplicitInputInheritance} -->

<!-- {=configurationWorkflowsSnippet} -->

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
choices = ["text", "json", "json-min"]
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

[[cli.publish-release.steps]]
name = "comment released issues"
type = "CommentReleasedIssues"

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

<!-- {/configurationWorkflowsSnippet} -->

CLI command interpolation variables:

<!-- {=configurationWorkflowVariables} -->

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

Performance tip: keep the default `monochange run release` path focused on built-in steps such as `PrepareRelease`. Arbitrary `Command` steps shell out to external tools, so expensive follow-up work like formatting, validation, publishing, or pushes should usually be gated behind an explicit input such as `when = "{{ inputs.commit }}"` if you want local release preparation to stay sub-second.

`RetargetRelease` is intentionally different from `PrepareRelease`-driven steps. It operates from git history plus source/provider information, discovers the durable `ReleaseRecord`, and then exposes structured `retarget.*` outputs for later command steps.

See [Repairable releases](./12-repairable-releases.md) for when to use `monochange step retarget-release` versus publishing a new patch release.

## GitHub release settings

Use `[source]` plus `[source.releases]` when you want command steps such as `PublishRelease` to derive repository release payloads from the prepared release. GitHub remains the default provider when `provider` is omitted. Add `[source.releases]` to restrict tag and publish operations to commits reachable from allowed release branches; `branches` accepts multiple names and glob patterns such as `release/*`.

<!-- {=configurationGitHubSnippet} -->

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

## Ecosystem settings

These settings are parsed from config and document intended control points for discovery:

<!-- {=configurationEcosystemSettingsSnippet} -->

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

## Changelog configuration

<!-- {=configurationPackageOverridesSnippet} -->

When `[defaults].package_type` is set, package entries may omit an explicit `type`.

Existing package and group changelogs support two appendable Markdown formats:

- `monochange` keeps the current heading-and-bullets layout
- `keep_a_changelog` renders section headings such as `### Features`, `### Fixes`, and `### Breaking changes`

Defaults can set a repository-wide changelog path pattern and format, while package and group changelog tables can override either field.

### Release-note streams and outputs

Streams separate the wording intended for different audiences without changing the changeset syntax. The built-in `default` stream always exists and preserves the current developer-oriented changelog behavior. A custom type opts into another stream with `stream`; types that omit it continue to use `default`.

Each changeset file resolves to exactly one stream. If one implementation needs both developer-facing detail and user-facing wording, author two small changesets and choose a type from each stream. This keeps each entry understandable on its own and prevents internal details from leaking into product notes.

```toml
[changelog.streams.user]
description = "Product release notes for app users"

[changelog.sections.native]
heading = "Native releases"
priority = 5

[changelog.sections.app_features]
heading = "App features"
priority = 10

[changelog.types.native]
bump = "major"
section = "native"

[changelog.types.app_feature]
bump = "minor"
section = "app_features"
stream = "user"

[changelog.outputs.user]
stream = "user"
format = "json"
mode = "release"
path = "{{ path }}/release-notes/{{ version }}.json"
targets = ["app"]

[source.releases]
source = "monochange"
changelog_output = "user"
```

This example makes `native` a major bump in the default stream and `app_feature` a minor bump in the user stream. A mobile app can use that distinction to require an app-store/native release for `native` changes while allowing an `app_feature` release through a patch system such as Shorebird.

Named `[changelog.outputs.<id>]` tables support:

| Field            | Meaning                                                                                             |
| ---------------- | --------------------------------------------------------------------------------------------------- |
| `stream`         | Stream to render; defaults to `default`                                                             |
| `targets`        | One or more configured package or group ids                                                         |
| `path`           | Destination template supporting `{{ path }}`, `{{ id }}`, and `{{ version }}`                       |
| `format`         | `monochange`, `keep_a_changelog`, `json`, or `text`                                                 |
| `mode`           | `append` for a cumulative Markdown changelog or `release` for a standalone current-release artifact |
| `initial_header` | Optional header for append mode; invalid with release mode                                          |

JSON and text outputs must use `mode = "release"`. The existing package/group changelog configuration is the implicit output named `default`; `[source.releases].changelog_output` selects which output becomes the hosted release body.

Preview or export one configured artifact without preparing the release:

```bash
# stdout
monochange notes --output user --target app

# explicit file (relative paths resolve from the workspace root)
monochange notes --output user --target app --file artifacts/app-release-notes.json
```

`--output` selects the configured stream and format. It is required so automation cannot accidentally publish the wrong audience. Use `--target` when an output has more than one target. The command is read-only: it does not update versions, consume changesets, or write the output's configured `path`. Omitting `--file` (or passing `--file -`) writes to stdout, so ordinary shell redirection also works.

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

## Package references

<!-- {=configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured ids.

Prefer package ids when a leaf package changed. That keeps the authored change as specific as possible, and monochange will still propagate bumps to dependents and synchronize any configured groups automatically.

Use a group id only when the change is intentionally owned by the whole group and should read that way in release output.

<!-- {/configurationPackageReferenceRules} -->

## Current status

<!-- {=configurationCurrentStatus} -->

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

## Validation

Run:

```bash
monochange step validate
```

`monochange step validate` validates:

- package and group declarations
- manifest presence for each package type
- group membership rules
- `versioned_files` structural rules (type/format/regex conflicts, required format fields, capture groups)
- `versioned_files` content checks: file existence, version field readability, regex pattern matching
- `.changeset/*.md` targets and overlap rules
- Cargo workspace version-group constraints
- `[source]` url scheme security (`https://` required)
