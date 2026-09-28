# monochange.toml configuration

`monochange.toml` is where you declare package ids, group release identities, changelog rendering, versioned files, source providers, lint rules, and custom CLI workflows.

Read the file before editing changesets or suggesting commands. Two repositories using monochange can have completely different package ids, group names, and workflow commands.

## Minimal configuration

The smallest useful config declares package ids and a default ecosystem:

```toml
[defaults]
# Ecosystem used by any [package.*] table that omits `type`.
package_type = "npm"
# Severity added to a dependent package when this package changes.
parent_bump = "patch"

[package."@acme/api"]
path = "packages/api"

[package."@acme/ui"]
path = "packages/ui"

[ecosystems.npm]
enabled = true
```

`path` is required and is relative to the workspace root. `type` is required unless `[defaults].package_type` supplies it.

Declare `type` per package when the repository mixes ecosystems:

```toml
[package."@acme/ui"]
path = "packages/ui"
type = "npm"

[package.acme_core]
path = "crates/acme_core"
type = "cargo"

[package.acme_cli]
path = "crates/acme_cli"
type = "cargo"
```

Supported types are `cargo`, `npm`, `deno`, `dart`, `python`, and `go`. The older `flutter` spelling still parses and normalizes to the `dart` ecosystem.

## Package fields

Every package accepts the following fields:

```toml
[package."@acme/api"]
path = "packages/api" # required, relative to the workspace root
type = "npm" # required unless [defaults].package_type is set
tag = false # skip creating a Git tag for this package
release = false # skip creating a provider release for this package
version_format = "namespaced" # tag shape; see "Version formats" below
version_source = "manifest" # "manifest" or "tag"; see "Tag-versioned packages"
bump_ceiling = "minor" # cap the severity classification may propose
classification_enforced = false # make classification advisory for this package
empty_update_message = "No direct changes; released with the group." # changelog fallback
release_title = "Acme API {{ version }}" # provider release title template
changelog_version_title = "v{{ version }}" # changelog heading template
ignore_ecosystem_versioned_files = false # skip this ecosystem's default versioned files
additional_paths = ["packages/api/**"] # extra globs that map changes to this package
ignored_paths = ["packages/api/docs/**"] # globs excluded from this package's changes
excluded_changelog_types = ["docs"] # note types that never appear in this changelog
versioned_files = ["Cargo.toml"] # extra files to version-stamp
```

`changelog`, `cli`, and `publish` take either a shorthand or a table. `changelog` accepts `true`, `false`, or a path string:

```toml
[package."@acme/api"]
path = "packages/api"
changelog = true # use {{ path }}/CHANGELOG.md
# changelog = false            # no changelog file for this package
# changelog = "docs/api.md"    # use this exact path
```

## Version formats

`version_format` decides the Git tag used for a release owner. The default, `namespaced`, produces collision-safe tags such as `@acme/api/v1.2.3`:

```toml
[package."@acme/api"]
path = "packages/api"
version_format = "namespaced" # @acme/api/v1.2.3
```

Use `primary` for the single release owner that should claim top-level tags:

```toml
[package.cli]
path = "crates/cli"
type = "cargo"
version_format = "primary" # v1.2.3
```

You can also supply a template. It must contain `{{ version }}`, and `{{ name }}`, `{{ ecosystem }}`, and `{{ repository }}` are also available:

```toml
[package.cli]
path = "crates/cli"
type = "cargo"
version_format = "{{ ecosystem }}/{{ name }}/v{{ version }}" # cargo/cli/v1.2.3
```

Only one package or group may use `primary`.

## Tag-versioned packages

By default, release planning reads the current version from the package manifest. Set `version_source = "tag"` when the version lives in the Git tag instead, which is common for GitHub Actions repositories and other packages whose manifest carries no version:

```toml
[package.web]
path = "."
type = "npm"
version_source = "tag"
initial_version = "0.1.0" # baseline when no matching tag exists yet
```

Without `initial_version`, a tag-versioned package with no reachable tag produces a warning and no release target.

## Floating tags

`floating_tags` declares moving aliases that `tag-release` force-moves to each non-prerelease release tag:

```toml
[package.cli]
path = "crates/cli"
type = "cargo"
version_format = "primary"
# After tagging v1.2.3, also move v1.2 and v1 to that commit.
floating_tags = ["v{{ major }}.{{ minor }}", "v{{ major }}"]
```

Alias templates support `{{ major }}`, `{{ minor }}`, `{{ patch }}`, plus the `version_format` variables. Floating tags are skipped for prereleases, never receive provider releases, and are excluded from baseline and previous-tag resolution.

## Bump propagation to dependents

`bump_propagation` declares what this target's own changes mean for the packages that depend on it:

```toml
[defaults]
# Workspace-wide fallback for dependents with no package or group declaration.
bump_propagation = "inherit"
bump_propagation_max = "major"

[package.core]
# Dependents match this package's severity, never exceeding minor.
bump_propagation = "inherit"
bump_propagation_max = "minor"

[package.tooling]
# Dependents always receive at least a minor bump.
bump_propagation = "minor"

[package.leaf]
# Dependents never release because of this package.
bump_propagation = "none"

[group.sdk]
packages = ["core"]
# A group declaration applies to members that declare nothing.
bump_propagation = "major"
```

`inherit` matches the target's own release severity, so breaking changes in the package mean breaking changes for its dependents. A fixed severity (`none`, `patch`, `minor`, `major`) is a floor. `bump_propagation_max` clamps inherit mode and is rejected without `bump_propagation = "inherit"`. Declarations resolve most-specific-first: package beats group beats `[defaults]`, and targets matching no declaration fall back to the legacy `[defaults].parent_bump` floor.

## Groups

A group gives several packages one outward version, tag, and changelog:

```toml
[group.sdk]
packages = ["@acme/api", "@acme/ui"]
tag = true # create a Git tag for the group
release = true # create a provider release for the group
version_format = "primary" # the group owns top-level v1.2.3 tags
changelog = { path = "CHANGELOG.md", format = "keep_a_changelog", include = "all" }
```

Groups accept the same release-identity fields as packages plus `packages`. Group members must already be declared under `[package.<id>]`, package and group ids share one namespace, and a package may belong to only one group. Group `tag`, `release`, and `version_format` override the member packages.

`changelog.include` filters which member-targeted changesets reach the group changelog:

```toml
[group.sdk.changelog]
path = "docs/sdk-changelog.md"
# "all" (default), "group-only", or an explicit list of member ids.
include = ["@acme/cli"]
```

With a list, a member-targeted changeset appears only when every target in that changeset is listed.

Use a group only for packages that genuinely release as one unit. A group collapses several package releases into a single outward release identity, so unrelated packages should stay out.

## Versioned files

`versioned_files` lists extra files that `PrepareRelease` should version-stamp. A bare string infers the package ecosystem:

```toml
[package.acme_core]
path = "crates/acme_core"
type = "cargo"
versioned_files = ["Cargo.toml"]
```

Use explicit entries for groups, for dependency sections, and for paths that need a glob:

```toml
[group.sdk]
packages = ["@acme/api", "@acme/ui"]
versioned_files = [
	# Stamp a nested manifest. Groups must use explicit entries because a group
	# can span ecosystems, so the ecosystem cannot be inferred.
	{ path = "package.json", type = "npm" },
	# Stamp a glob-matched file at any depth.
	{ path = "**/install.sh", regex = 'SDK_VERSION="(?<version>\d+\.\d+\.\d+)"' },
]
```

Typed entries can also rewrite dependency ranges. `prefix` controls the range operator:

```toml
versioned_files = [
	# Write internal npm dependencies as tilde ranges, for example "~1.2.3".
	{ path = "package.json", type = "npm", fields = ["dependencies"], prefix = "~" },
	# Write a bare version into one field of a TOML manifest.
	{ path = "Cargo.toml", type = "cargo", fields = ["workspace.metadata.bin.monochange.version"], prefix = "" },
]
```

Accepted prefixes are `^`, `~`, `>=`, `=`, `v`, and `""`. Without one, the ecosystem default applies: `^` for npm, deno, and dart, `>=` for python, `v` for go, and empty for cargo. Override the ecosystem default with `[ecosystems.<name>] dependency_version_prefix`. The prefix affects internal dependency references only; a package's own `version` field is always written bare. `format` and `regex` entries do not accept `prefix`.

Use `format` for structured files that should not receive ecosystem-specific handling:

```toml
[package.core]
path = "crates/core"
versioned_files = [
	{ path = "metadata.json", format = "json", fields = ["release.version"] },
	{ path = "tools.toml", format = "toml", fields = ["tool.sdk.version"] },
	{ path = "pubspec-overrides.yaml", format = "yaml", fields = ["metadata.sdkVersion"] },
	{ path = ".env", format = "env", fields = ["VERSION"] },
]
```

`format` accepts `json`, `toml`, `yaml`, `yml`, and `env`. `fields` is required and must name every value to update; monochange infers no defaults in format mode. JSON, TOML, YAML, and YML fields use dot-separated paths. Env fields use exact keys and update existing `KEY=value` or `export KEY=value` lines. Format entries cannot set `type` or `regex`.

Use `regex` to stamp plain text such as README badges and install scripts. The pattern must contain a named `version` capture group, and `path` may be a glob:

```toml
[package.core]
path = "crates/core"
versioned_files = [
	# A download link in the README.
	{ path = "README.md", regex = 'https://example\.com/download/v(?<version>\d+\.\d+\.\d+)\.tgz' },
	# A shields.io version badge.
	{ path = "README.md", regex = 'img\.shields\.io/badge/version-(?<version>\d+\.\d+\.\d+)-blue' },
]

[ecosystems.cargo]
versioned_files = [
	# A workspace-wide version constant in Rust source.
	{ path = "crates/constants/src/lib.rs", regex = 'pub const VERSION: &str = "(?<version>\d+\.\d+\.\d+)"' },
]
```

Regex entries cannot set `type`, `prefix`, `fields`, or `name`, because they operate on raw text.

## Ecosystem settings

```toml
[ecosystems.cargo]
enabled = true
roots = ["crates/*"] # globs to scan for manifests
exclude = ["crates/experimental/*"]
dependency_version_prefix = "^"
versioned_files = ["Cargo.toml"]
lockfile_commands = [
	# `cwd` is relative to the workspace root; `shell` is false, true, or a shell path.
	{ command = "cargo generate-lockfile" },
]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
	{ command = "npm install --package-lock-only", cwd = "packages/legacy", shell = true },
]
```

Each `lockfile_commands` entry is a table with `command`, optional `cwd`, and optional `shell`. A bare string is rejected. `shell = false` runs the executable directly, `shell = true` runs through `sh -c`, and `shell = "bash"` uses that binary.

Configuring `lockfile_commands` for an ecosystem replaces monochange's built-in direct lockfile rewrite for that ecosystem, so the commands own lockfile refresh entirely.

## Rust semantic compatibility

Cargo packages can opt into cargo-semver-checks when `change classify` runs at the semantic detection level:

```toml
[ecosystems.cargo.semver_checks]
enabled = true
timeout_seconds = 300

[[ecosystems.cargo.semver_checks.matrix]]
name = "default"
feature_mode = "default"

[[ecosystems.cargo.semver_checks.matrix]]
name = "all-features"
feature_mode = "all"

[[ecosystems.cargo.semver_checks.matrix]]
name = "wasm"
feature_mode = "none"
features = ["wasm"]
target = "wasm32-unknown-unknown"
```

`feature_mode` accepts `default`, `all`, `none`, or `heuristic`. Use `baseline_features` and `current_features` instead of `features` when a feature was renamed and each endpoint needs a different name. Matrix names must be unique, the matrix allows 1 to 16 cells, and each cell's timeout is 1 to 1800 seconds. This table is rejected for non-Cargo ecosystems.

The matrix runs only during `monochange change classify --detection-level semantic`. It executes Cargo builds, including build scripts and procedural macros, so do not enable it for untrusted code running with elevated credentials.

## Publishing settings

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin" # "builtin" or "external"
registry = "npm"
trusted_publishing = true
fail_on_duplicate = false # fail instead of skipping an already-published version

[ecosystems.npm.publish.timeout]
timeout_seconds = 300 # 0 disables the per-package publish timeout
retries = 2 # retries after a timeout before marking the package failed

[ecosystems.npm.publish.rate_limits]
enforce = true # block a publish run that exceeds one registry window

[ecosystems.npm.publish.placeholder]
readme_file = "docs/web-placeholder.md" # workspace-relative placeholder README

[package."@acme/private-tool"]
path = "tools/private-tool"
type = "npm"
# Version this package but never publish it.
publish = { enabled = false }

[package."@acme/custom-registry"]
path = "packages/custom-registry"
type = "npm"
# Keep planning and release records, but let another CI job publish.
publish = { enabled = true, mode = "external" }
```

`placeholder.readme` and `placeholder.readme_file` are mutually exclusive, and validation rejects setting both.

Package-level publish settings override the ecosystem defaults, so configure shared trusted-publishing and attestation policy once on the ecosystem. `trusted_publishing` is either a boolean or a table, never both, so the table form already enables it:

```toml
[ecosystems.npm.publish.trusted_publishing]
repository = "acme/widgets"
workflow = "publish.yml"
environment = "publisher"

[package.cli]
path = "packages/cli"
type = "npm"
# Publish this package through a different workflow.
[package.cli.publish.trusted_publishing]
workflow = "publish-cli.yml"

[package.legacy]
path = "packages/legacy"
type = "npm"
# Opt this package out of trusted publishing entirely.
[package.legacy.publish]
trusted_publishing = false
```

Use the boolean shorthand when monochange should infer the repository, workflow, and environment from the GitHub Actions context:

```toml
[ecosystems.npm.publish]
trusted_publishing = true
```

`publish.attestations.require_registry_provenance` is separate from `trusted_publishing`: enable trusted publishing, then require registry-native provenance where the ecosystem supports it.

```toml
[ecosystems.npm.publish.attestations]
require_registry_provenance = true

[package.legacy]
path = "packages/legacy"
type = "npm"

# Use the table form rather than `publish = { ... }` when a nested table such as
# `attestations` also needs to be set, because an inline table cannot be extended.
[package.legacy.publish]
enabled = true
mode = "external"

# This package publishes outside monochange, so the requirement does not apply.
[package.legacy.publish.attestations]
require_registry_provenance = false
```

Only npm and JSR treat provenance as enforceable. PyPI, crates.io, pub.dev, and Go proxy publishing reject this requirement, because monochange cannot verify equivalent registry-native attestations for those flows.

Built-in publishing targets the canonical public registry for each ecosystem: Cargo to `crates.io`, npm to `npm`, Deno to `jsr`, Dart and Flutter to `pub.dev`, Python to `pypi`, and Go modules to a proxy through VCS tags. Use `mode = "external"` for private registries and custom release jobs.

## Changelog configuration

```toml
[defaults.changelog]
path = "{{ path }}/CHANGELOG.md"
format = "keep_a_changelog"
initial_header = """
# Changelog

All notable changes to this project will be documented in this file.
"""

[changelog.types]
feat = { bump = "minor", section = "features", description = "New user-facing functionality" }
fix = { bump = "patch", section = "fixes", description = "Bug fixes" }
docs = { bump = "none", section = "docs", description = "Documentation only" }

[changelog.sections]
features = { heading = "Features", priority = 20 }
fixes = { heading = "Fixes", priority = 30 }
docs = { heading = "Documentation", priority = 40 }
```

`initial_header` is rendered only when a changelog file is created from empty content. Existing preambles are preserved. Section `priority` decides ordering and how the changelog collapses low-priority sections:

```toml
[changelog.section_thresholds]
# Collapse sections whose priority is at or above this value.
collapse = 40
# Omit sections whose priority is above this value. Must be at least `collapse`;
# it defaults to the maximum, so setting `collapse` alone is valid.
ignored = 60
```

Style options control how entries render:

```toml
[changelog.style]
# "inline" (default), "blockquote", "plain", or "omit".
metadata_style = "inline"
# Prefix each package label with a colored symbol for its bump.
package_bump_symbols = true
# "after_heading" (default) or "after_change".
package_label_placement = "after_heading"
# "inline" (default), "badge", or "omit".
package_label_style = "inline"
# "blank_line" (default), "thematic_break", or "none".
section_separator = "blank_line"
# "details" (default) wraps collapsed sections in HTML details, "plain" does not.
collapsed_section_style = "details"
```

Use `[changelog.release_notes]` to override any of those for hosted release notes without changing the changelog file:

```toml
[changelog.release_notes]
metadata_style = "omit"
```

### Release-note streams and outputs

Streams separate wording for different audiences without changing changeset syntax. The `default` stream always exists, so only additional audiences need declaring:

```toml
[changelog.streams.user]
description = "Product release notes"

[changelog.types.native]
bump = "major"
section = "breaking"

[changelog.types.app_feature]
bump = "minor"
section = "features"
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

Each changeset file resolves to exactly one stream. When one implementation needs both developer detail and user-facing wording, write two changesets rather than mixing audiences in one file, then run `monochange step validate` to catch a file whose targets cross streams.

`[changelog.outputs.<id>]` supports `stream`, `targets`, `path`, `format`, `mode`, and `initial_header`. Formats are `monochange`, `keep_a_changelog`, `json`, and `text`; JSON and text require `mode = "release"`, while `append` maintains a cumulative Markdown file. A package or group changelog is the implicit output named `default`.

A mobile repository can use a major `native` type to require app-store binaries and a minor `app_feature` type for patch-deliverable features. Decide from the configured type rather than from file names, then confirm the choice in the dry-run manifest.

## Package CLI registration

A package that ships a CLI can register it under `[package.<id>].cli`. Registration is additive: library and registry surface analysis continues unchanged, and change classification additionally diffs the CLI's command surface against a committed baseline.

```toml
[package.monochange]
path = "crates/monochange"
cli = { name = "monochange", snapshot = "monochange snapshot --view index" }
```

- `name` is the binary name users invoke. It also names the committed baseline at `.monochange/cli-snapshots/<name>.json` and must be unique in the workspace.
- `snapshot` is required and names a command that prints a normalized command-surface snapshot JSON document on stdout. It accepts a string or a table with `command`, `cwd`, and `shell`.

Refresh the baseline during release preparation with `monochange snapshot --package <id> --save`, and inspect registrations with `monochange snapshot --list`.

Rust and clap CLIs get the document from `monochange snapshot --view index`. For TypeScript, Python, Go, and Dart CLIs, an emitter script maps the framework's command metadata into the same shape described by the schema at <https://monochange.github.io/monochange/schemas/command-snapshot.schema.json>. Two fields are fixed by that contract: `kind` is always `"cli-surface"`, and `schema_version` must match the version this monochange build supports. The validator rejects unknown fields, so a misspelled field fails rather than silently dropping data. When a framework exposes no structured metadata, help-text inference works but must be marked `"confidence": "low"`.

Classification then reports command-surface breaks instead of unclassified changes, proposing `major` for removed commands or options and `minor` for additions. Skip the comparison with `--skip-cli-snapshots` or `MONOCHANGE_SKIP_CLI_SNAPSHOTS=1`.

## Custom CLI workflows

A `[cli.<name>]` table creates `monochange run <name>` in that repository. These workflows compose built-in steps with local shell commands, input defaults, dry-run behavior, and CI guards.

```toml
[cli.change]
help_text = "Create a changeset"
inputs = [
	{ name = "package", type = "string_list", required = true },
	{ name = "bump", type = "choice", choices = ["none", "patch", "minor", "major"], default = "patch" },
	{ name = "reason", type = "string", required = true },
	{ name = "type", type = "string" },
	{ name = "caused_by", type = "string_list" },
]
steps = [
	# Command inputs are not inherited. List every input this step should see.
	{ name = "create change file", type = "CreateChangeFile", inputs = ["package", "bump", "type", "caused_by", "reason"] },
]

[cli.release]
help_text = "Prepare versioned package files"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "text" },
	{ name = "refresh", type = "boolean", default = false },
]
steps = [
	{ name = "plan release", type = "PrepareRelease", inputs = ["format"] },
	# `when` reads the same explicit input context, so `refresh` must be listed above.
	{ name = "refresh lockfiles", type = "Command", command = "pnpm install --lockfile-only", when = "{{ inputs.refresh }}" },
]
```

Name workflow commands after user intent (`change`, `release`, `publish-check`) rather than implementation detail, and expose safe dry-run or JSON-producing workflows for automation.

Step inputs accept native TOML literals. Booleans stay booleans and are stringified when the step runs, and numbers are coerced at parse time, so `{ jobs = 4 }` and `{ jobs = "4" }` are equivalent:

```toml
[[cli.release.steps]]
name = "publish"
type = "Command"
command = "npm publish --jobs {{ inputs.jobs }}"
inputs = { jobs = 4, dry_run = true }
```

Add an `interactive` boolean input and pass it to a `Command` step when the command needs to own the terminal. The step inherits stdio for that run, so prompts and terminal UIs work:

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

Interactive steps capture no output, so `steps.<id>.stdout` and `steps.<id>.stderr` stay empty and downstream steps cannot read what the command printed.

Validate custom workflows with `monochange step validate` and inspect them with `monochange help`.
