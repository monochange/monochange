# Manifest linting with `monochange check`

monochange can lint monorepo package manifests through `monochange check`, using rules configured under `[lints]` in `monochange.toml`.

<!-- {=lintingPolicyReference} -->

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
| `changesets/recommended` | Baseline changeset hygiene.                                     | `changesets/summary = error`                                                                                                                                                                                                                                                                                                            |
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
| `npm/workspace-protocol`                         | npm-family     | correctness   | yes     | Requires internal dependencies to use `workspace:` ranges.                                                                          |
| `npm/sorted-dependencies`                        | npm-family     | style         | yes     | Sorts dependency sections alphabetically.                                                                                           |
| `npm/required-package-fields`                    | npm-family     | correctness   | no      | Requires selected `package.json` metadata fields.                                                                                   |
| `npm/root-no-prod-deps`                          | npm-family     | best practice | yes     | Keeps production dependencies out of the workspace root package.                                                                    |
| `npm/no-duplicate-dependencies`                  | npm-family     | correctness   | yes     | Prevents the same dependency from appearing in multiple dependency sections.                                                        |
| `npm/unlisted-package-private`                   | npm-family     | correctness   | yes     | Requires unmanaged packages to set `private: true`.                                                                                 |
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

- `required` — require the summary heading.
- `heading_level` — require a Markdown heading level from `1` to `6`.
- `min_length` / `max_length` — constrain summary text length.
- `forbid_trailing_period` — reject summaries ending in `.`.
- `forbid_conventional_commit_prefix` — reject summaries such as `feat: add parser`.
- `require_description` — require a non-empty paragraph after the heading.

### `changesets/no_section_headings`

**Why:** change types already come from the changeset entries. Repeating them as body headings creates noisy generated changelogs.

**With the rule:** headings that duplicate configured changelog types, such as `## Breaking` or `## Fix`, are rejected.

### `changesets/bump/<severity>`

**Why:** different bump severities can require different explanation standards. A `major` bump often needs impact and migration notes, while a `patch` bump may only need a concise description.

**Supported severities:** `none`, `patch`, `minor`, and `major`.

**Useful options:**

- `required_sections` — headings that must appear in the body.
- `forbidden_headings` — headings that must not appear in the body.
- `min_body_chars` / `max_body_chars` — body length bounds.
- `require_code_block` — require a fenced code block.
- `required_bump` — require entries governed by this rule to use a specific bump severity.

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

- `fix` — defaults to `true`; rewrites the dependency entry when safe.

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

- `require_workspace` — defaults to `true`; require internal dependencies to use `workspace = true`.
- `fix` — defaults to `true`; rewrites safe internal dependency entries.

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

- `fields` — replace the default required-field list.

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

- `fix` — defaults to `true`; rewrites dependency sections in sorted order.

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

- `fix` — defaults to `true`; inserts `publish = false` when safe.

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

- `require_for_private` — defaults to `false`; also enforce the rule for private packages.
- `fix` — defaults to `true`; rewrites internal dependency ranges to `workspace:` ranges.

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

- `fix` — defaults to `true`; rewrites dependency sections in sorted order.

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

- `fields` — replace the default required-field list.

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

- `fix` — defaults to `true`; moves root `dependencies` into `devDependencies`.

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

- `fix` — defaults to `true`; removes duplicate entries from later sections.

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

- `fix` — defaults to `true`; inserts `private: true` when safe.

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

- `minimum` — override the minimum lower bound for your workspace.
- `require_upper_bound` — set to `false` if your policy intentionally omits an upper bound.

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

- `fix` — defaults to `true`; rewrites dependency sections in sorted order.

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

- `fields` — replace the default required-field list.

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

- `allow` — list dependency names that may use `git:` sources.

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

- `fix` — defaults to `true`; inserts `publish_to: none` when safe.

### `dart/no-unexpected-dependency-overrides`

**Why:** `dependency_overrides` are sometimes necessary, but they should usually be limited to private packages or a small allow list of explicitly approved packages.

**With the rule:** monochange reports `dependency_overrides` unless they are allowed by privacy or package name.

**Options:**

- `allow_for_private` — defaults to `true`; allow overrides in private packages.
- `allow_packages` — list package names that may keep `dependency_overrides`.

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

- `mode` — choose `"path"` or `"hosted"` for packages that do not use `resolution: workspace`.

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

- `fix` — defaults to `true`; rewrites Flutter assets and fonts in sorted order.

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
