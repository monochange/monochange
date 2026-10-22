# Ecosystems

monochange uses ecosystem adapters to translate native package-manager files into one release-planning model. Each adapter answers the same questions:

- which package manifests exist in the repository?
- which packages depend on other packages?
- where should a package version and internal dependency references be rewritten during a release?
- should lockfiles be rewritten directly, refreshed with a command, or left to external tooling?
- can monochange publish the package directly, or should publication stay external?

## Capability matrix

| Ecosystem      | Package type      | Discovery sources                                                                                        | Version and dependency updates                                                               | Lockfile behavior                                                                                                                                                                                                             | Built-in registry publishing                       |
| -------------- | ----------------- | -------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Cargo          | `cargo`           | `Cargo.toml` workspaces and standalone crates                                                            | `Cargo.toml` package versions and internal dependency requirements                           | Direct `Cargo.lock` rewrite by default; configure `cargo generate-lockfile`, `cargo check`, or another command when you need package-manager resolution                                                                       | `crates.io`                                        |
| npm-family     | `npm`             | npm workspaces, pnpm workspaces, Yarn workspaces, Bun workspaces, and standalone `package.json` packages | `package.json` versions and dependency ranges                                                | Direct `package-lock.json`, `pnpm-lock.yaml` (including pnpm 12 two-document lockfiles), `yarn.lock` (Classic and Berry), `bun.lock`, and `bun.lockb` updates by default; command overrides support package-manager refreshes | `npm`                                              |
| Deno           | `deno`            | Deno workspaces and standalone `deno.json` / `deno.jsonc` packages                                       | Deno manifest versions, exports/imports metadata, and dependency references                  | Direct `deno.lock` update when possible; no inferred lockfile command                                                                                                                                                         | `jsr`                                              |
| Dart / Flutter | `dart`, `flutter` | Dart and Flutter workspaces plus standalone `pubspec.yaml` packages                                      | `pubspec.yaml` versions and dependency ranges                                                | Direct `pubspec.lock` update by default; configure `dart pub get` or `flutter pub get` when you need full solver refreshes                                                                                                    | `pub.dev`                                          |
| Python         | `python`          | uv workspaces, Poetry projects, and standalone `pyproject.toml` packages                                 | PEP 621 `[project]` and Poetry `[tool.poetry]` package versions plus dependency specifiers   | Does not mutate `uv.lock` or `poetry.lock` directly; infers `uv lock` and `poetry lock` commands; unknown Python lockfiles are skipped                                                                                        | `pypi`                                             |
| Go             | `go`              | Standalone `go.mod` modules                                                                              | Internal `require` directives in `go.mod`; package versions stay in VCS tags                 | Does not mutate `go.sum` directly; infers `go mod tidy` so the Go toolchain refreshes `go.mod` and checksum data                                                                                                              | Go module proxy via VCS tags                       |
| GitHub Actions | `github_actions`  | Configured `[package.<id>]` declarations with `action.yml` (or `action.yaml`) in the package directory   | None: releases are git tags; a sibling `package.json` `version` field is synced when present | Not applicable, no lockfiles                                                                                                                                                                                                  | None: the tag and GitHub release are the publish   |
| Swift          | `swift`           | `Package.swift` manifests                                                                                | None: `Package.swift` carries no version field; releases are repository tags                 | Not applicable, no lockfiles                                                                                                                                                                                                  | None: the tag and provider release are the publish |
| CocoaPods      | `cocoapods`       | `.podspec` and `.podspec.json` manifests                                                                 | `s.version` declarations in podspecs, including JSON podspecs                                | Not applicable, no lockfiles                                                                                                                                                                                                  | `cocoapods_trunk` (the CocoaPods trunk)            |
| Gradle         | `gradle`          | `gradle.properties` manifests declaring `VERSION_NAME`                                                   | `VERSION_NAME` (and other listed keys) in `gradle.properties`                                | Not applicable, no lockfiles                                                                                                                                                                                                  | `maven_central` (Sonatype Central Portal)          |

The built-in publishing column is intentionally narrower than release planning. It lists only the canonical public registry for each supported ecosystem; private registries and custom publication flows should use `mode = "external"`.

## Shared behavior across ecosystems

All supported ecosystems feed the same planner. After discovery, monochange can:

- render package ids and manifest paths relative to the repository root
- normalize dependency edges into one graph
- apply `[group.<id>]` version synchronization rules
- propagate dependent bumps through internal dependency edges
- update native manifests during `monochange run release`
- update extra `versioned_files` entries, including regex-managed files
- render changelogs and release notes from `.changeset/*.md`
- create durable release records and post-merge tags

`[ecosystems.<name>]` configuration controls settings such as dependency-version prefixes, extra versioned files, publish defaults, and lockfile commands. Discovery still scans every supported ecosystem regardless of `[ecosystems.*].enabled`, `roots`, or `exclude` toggles.

## Syncing internal dependency versions

`monochange versions sync` updates existing internal dependency references outside a release. (Bare `monochange versions` only reports what `sync` would change and never writes files.) It discovers workspace package versions, finds supported manifests that reference another workspace package, and rewrites those references to the canonical package version. Use `--dry-run` first to print the planned edits without writing files.

```nu
monochange versions sync --dry-run
monochange versions sync --strategy exact
```

The `--strategy` flag accepts `default`, `exact`, `caret`, or `compatible`. `default` uses each supported ecosystem's normal constraint style; for `monochange versions` that means each ecosystem's configured or default constraint style. Dart version sync scans `dependencies`, `dev_dependencies`, and `dependency_overrides`; when a pubspec uses `resolution: workspace`, path references to internal packages are converted to version constraints. npm version sync scans package dependency sections and leaves `workspace:*` protocol references alone. Cargo, Deno, Go, and Python manifests are also rewritten when they reference another workspace package.

[Internal dependency versions](../reference/versions.md) lists the exact constraint each strategy writes per ecosystem. `versions sync` cannot write custom prefixes such as `~` or `=`; to stamp internal dependency references with those, use a typed `versioned_files` entry with an explicit `prefix`, or set `[ecosystems.<name>] dependency_version_prefix` to change the prefix typed versioned files write by default (see [Versioned files](04-configuration.md#versioned-files)).

## Cargo

Cargo support is designed for Rust crates that keep version data in `Cargo.toml` and dependency resolution in `Cargo.lock`.

Use Cargo support when your repository has:

- a root Cargo workspace with `members`
- standalone crates outside a workspace
- internal crate dependencies that should move together when one crate is released
- crates published to `crates.io`

Cargo-specific behavior:

- package ids come from each crate manifest
- dependency references use Cargo's native requirement style; the default dependency version prefix is empty
- `Cargo.lock` is updated directly by default for fast release preparation
- incomplete or complex lockfile cases can be delegated to explicit lockfile commands
- built-in publishing targets `crates.io`
- publish readiness validates common crates.io requirements, including `publish`, `description`, and license metadata

## npm, pnpm, Yarn, and Bun

The npm-family adapter covers JavaScript and TypeScript packages that share `package.json` as their manifest format.

Use npm-family support when your repository has:

- npm workspaces declared in `package.json`
- pnpm workspaces declared in `pnpm-workspace.yaml`
- Yarn Classic or Yarn Berry workspaces with `yarn.lock` lockfiles
- Bun workspaces and Bun lockfiles
- standalone `package.json` packages
- internal workspace dependencies that use npm-compatible version ranges

npm-family behavior:

- package ids come from `package.json` names
- internal dependency ranges default to the `^` prefix
- `dependencies`, `devDependencies`, and `peerDependencies` participate in dependency updates
- `monochange versions sync` can repair npm internal dependency ranges outside a release while preserving `workspace:*` protocol references
- direct lockfile support covers `package-lock.json`, `pnpm-lock.yaml` (including pnpm 12 lockfiles written as two YAML documents), `yarn.lock` (Classic and Berry), `bun.lock`, and `bun.lockb`
- direct rewrites move resolved version pins of released workspace packages and leave `workspace:` and `link:` protocol references untouched; configure `lockfile_commands` when you need package-manager resolution for checksums or entry keys
- built-in publishing targets the public `npm` registry
- GitHub npm trusted-publishing diagnostics are built in; registry-side enrollment stays manual or external, and trusted npm publishes use the `npm` CLI directly

## Deno

Deno support is for packages described by `deno.json` or `deno.jsonc`, including workspaces that publish to JSR.

Use Deno support when your repository has:

- `deno.json` or `deno.jsonc` package manifests
- Deno workspace members
- `imports` entries that connect internal packages
- packages published to `jsr`

Deno behavior:

- internal dependency ranges default to `^`
- `imports` are the primary dependency field
- `deno.lock` can be updated directly when present
- monochange does not infer a default Deno lockfile command; configure one if your release flow needs `deno cache`, `deno task`, or another resolver step
- built-in publishing targets `jsr`

## Dart and Flutter

Dart and Flutter share the canonical `dart` ecosystem settings because both use `pubspec.yaml` and Pub's version constraints. The legacy `flutter` config and CLI filter spelling is still accepted as an alias, but normalized package records use the Dart ecosystem with Flutter metadata.

Use Dart / Flutter support when your repository has:

- pure Dart packages
- Flutter packages with a `flutter` section
- Dart or Flutter workspace layouts
- packages published to `pub.dev`

Dart / Flutter behavior:

- package type is canonically `dart` for both pure Dart and Flutter packages; Flutter packages are detected from `pubspec.yaml` metadata and published with `flutter pub publish` when appropriate
- internal dependency ranges default to `^`
- `dependencies` and `dev_dependencies` participate in dependency updates
- `monochange versions sync` can repair Dart internal dependency ranges outside a release, including converting internal `path:` references to version constraints when `resolution: workspace` is enabled
- `pubspec.lock` can be rewritten directly by default
- configure `dart pub get` or `flutter pub get` as lockfile commands when you need the Pub solver to refresh files instead of the direct updater
- built-in publishing targets `pub.dev`

## Python

Python support is centered on `pyproject.toml`. It covers modern PEP 621 projects, Poetry projects, uv workspaces, and standalone packages discovered by scanning for manifests.

Use Python support when your repository has:

- a uv workspace declared under `[tool.uv.workspace]`
- PEP 621 package metadata under `[project]`
- Poetry package metadata under `[tool.poetry]`
- standalone Python packages with `pyproject.toml`
- internal dependencies that should receive version bumps alongside released workspace packages

Python discovery works in two passes:

1. If the repository root has a `pyproject.toml` with uv workspace members, monochange expands the member globs and reads each member manifest.
2. monochange then scans for standalone `pyproject.toml` files that were not already included by the uv workspace pass.

When a manifest has both PEP 621 and Poetry metadata, monochange prefers `[project]`. If `[project].dynamic` contains `"version"`, monochange treats the package version as dynamic and does not rewrite the version field.

Python version and dependency behavior:

- package names and dependency names are normalized using Python's PEP 503 style normalization for dependency graph matching
- PEP 440 versions are parsed into the shared semantic-version model when possible
- internal dependency ranges default to the `>=` prefix
- PEP 621 `dependencies` are runtime dependencies
- PEP 621 `optional-dependencies` are development/optional dependency edges for release-planning purposes
- Poetry `dependencies` are runtime dependencies, except the special `python` constraint is skipped
- Poetry dependency groups under `[tool.poetry.group.<name>.dependencies]` are development dependencies
- release preparation rewrites `pyproject.toml` package versions and matching dependency specifiers while preserving extras such as `httpx[cli]`

Python lockfile behavior is command-based by design:

<!-- {=pythonLockfileCommandInference} -->

For Python projects, monochange infers package-manager commands instead of mutating lockfiles directly: `uv.lock` uses `uv lock`, and `poetry.lock` uses `poetry lock`. Poetry 2 preserves existing locked versions by default; its removed `--no-update` option must not be added. Unknown Python lockfile names are skipped rather than guessed.

<!-- {/pythonLockfileCommandInference} -->

Configuring `[ecosystems.python].lockfile_commands` overrides the inferred commands.

Built-in Python publishing targets PyPI. monochange builds Python artifacts with `uv build --out-dir dist` and publishes them with `uv publish`, using `--trusted-publishing always` when trusted publishing is enabled and `--trusted-publishing never` otherwise. Placeholder publishing creates a minimal Hatchling project with a normalized module directory under `src/`.

Example Python package configuration:

```toml
[package.api]
path = "services/api"
type = "python"
changelog = true

[package.api.publish]
enabled = true
mode = "builtin"
registry = "pypi"
trusted_publishing = true

[ecosystems.python]
dependency_version_prefix = ">="
# Optional: override inferred uv/Poetry lockfile commands.
lockfile_commands = [{ command = "uv lock", cwd = "." }]
```

## Go

Go support is centered on `go.mod` files. Go module versions live in VCS tags rather than manifest fields, so monochange updates internal `require` directives and records enough metadata for tag-based publishing.

Use Go support when your repository has:

- one or more modules declared by `go.mod`
- multi-module layouts where submodules need path-prefixed tags such as `api/v1.2.3`
- internal module dependencies that should receive version bumps alongside released workspace modules
- packages published through normal Go module proxy discovery

Go behavior:

- package ids come from the module path in the `module` directive
- internal dependency ranges default to exact Go module versions with a leading `v`, matching Go module semantics
- `require` directives participate in dependency updates, including grouped `require (...)` blocks
- Go v2+ semantic import versioning remains encoded in module paths, not a separate manifest version field
- release planning resolves each module's current version from the highest semantic version among repository tags matching its release owner's `version_format`, so `prepare` and `preview` can plan Go releases without a manifest version field
- `go.sum` is treated as checksum data, not as a lockfile to patch directly
- monochange infers `go mod tidy` when `go.mod` / `go.sum` changes need package-manager refreshes
- built-in publishing creates VCS tags: root modules use `v1.2.3`, while submodules use path-prefixed tags such as `api/v1.2.3`
- readiness and publish checks query the Go module proxy for `<module>/@v/<version>.info` visibility

Example Go package configuration:

```toml
[package.api]
path = "services/api"
type = "go"
changelog = true

[package.api.publish]
enabled = true
mode = "builtin"
registry = "go_proxy"
trusted_publishing = false

[ecosystems.go]
# Optional: override inferred tidy commands.
lockfile_commands = [{ command = "go mod tidy", cwd = "services/api" }]
```

## Swift

Swift support targets Swift Package Manager packages declared by `Package.swift`. The Swift Package Manager resolves versions from repository tags, so Swift packages release through the tag and provider-release steps rather than a registry publish.

Use Swift support when your repository has:

- Apple platform packages declared by `Package.swift`
- bindings or wrappers that should release in lockstep with the rest of a mixed-language monorepo

Swift behavior:

- package names come from the `name:` argument of the `Package(…)` initializer
- `Package.swift` carries no version field: release planning resolves each package's current version from repository tags, exactly like Go
- `type = "swift"` versioned-file entries validate but never rewrite `Package.swift`; releases are the group's tag
- publishing stays with the release flow: create the tag and provider release, and Swift consumers resolve it

Example Swift package configuration:

```toml
[package.apple]
path = "platforms/apple"
type = "swift"

[group.sdk]
packages = ["apple"]
tag = true
release = true
version_format = "primary"
```

## CocoaPods

CocoaPods support targets pods declared by `.podspec` (Ruby) or `.podspec.json` manifests and publishes them to the CocoaPods trunk.

CocoaPods plans a read-only trunk trial for November 1–7, 2026 and a permanent switch on December 2, 2026. See the [official trunk read-only plan](https://blog.cocoapods.org/CocoaPods-Specs-Repo/) for schedule changes. Discovery, release planning, and podspec updates remain useful after trunk stops accepting publications; choose an external publishing flow for a different distribution service.

Use CocoaPods support when your repository has:

- pods consumed through a `Podfile`, including React Native and Lynx apps that autolink native pods
- podspecs whose version should move in lockstep with the rest of the workspace

CocoaPods behavior:

- pod names and versions come from the `s.name` and `s.version` declarations; podspecs that compute their version at evaluation time carry no literal version and fall back to tags and group versions
- `type = "cocoapods"` versioned-file entries rewrite `s.version` (or the JSON `version` field), preserving the original quote style
- built-in publishing runs `pod trunk push <podspec> --allow-warnings`; dry runs swap to `pod spec lint` so the same validation runs without mutating the trunk
- duplicate-version detection queries the trunk API (`trunk.cocoapods.org/api/v1/pods/<name>`)
- placeholder name reservation is unsupported because trunk validation needs real, downloadable pod sources
- the trunk authenticates through per-machine session tokens rather than trusted publishing: set `trusted_publishing = false` and export `COCOAPODS_TRUNK_TOKEN` from a registered trunk session in CI

Example CocoaPods package configuration:

```toml
[package.canvist_pod]
path = "platforms/apple"
type = "cocoapods"

[ecosystems.cocoapods.publish]
enabled = true
mode = "builtin"
registry = "cocoapods_trunk"
trusted_publishing = false
```

## Gradle

Gradle support targets Android and JVM libraries that publish to Maven Central, discovered through the `VERSION_NAME` convention in `gradle.properties`.

Use Gradle support when your repository has:

- Kotlin or Java libraries built with Gradle and published through the Sonatype Central Portal
- Android bindings whose `VERSION_NAME` should move in lockstep with the rest of the workspace

Gradle behavior:

- a project qualifies when its `gradle.properties` declares `VERSION_NAME`; build-only properties files are ignored, as are `build/` outputs
- package names are the Maven coordinate `group:artifact` when `GROUP` (or `POM_GROUP_ID`) is set, with the artifact id taken from `POM_ARTIFACT_ID`, the `rootProject.name` of the settings script, or the directory name
- `type = "gradle"` versioned-file entries rewrite listed keys such as `VERSION_NAME`, preserving indentation, separators, line endings, and unrelated lines
- built-in publishing runs `./gradlew publishAndReleaseToMavenCentral`, the `com.vanniktech.maven.publish` task that uploads and releases a Central Portal deployment in one run; dry runs append `--dry-run` to display the task graph without executing tasks
- duplicate-version detection reads `maven-metadata.xml` from `repo1.maven.org` for the resolved coordinate
- Maven Central has no trusted publishing: set `trusted_publishing = false` and provide a Central Portal user token (`ORG_GRADLE_PROJECT_mavenCentralUsername` / `ORG_GRADLE_PROJECT_mavenCentralPassword`) plus an in-memory signing key in CI

Example Gradle package configuration:

```toml
[package.canvist_android]
path = "platforms/android"
type = "gradle"

[ecosystems.gradle.publish]
enabled = true
mode = "builtin"
registry = "maven_central"
trusted_publishing = false
```

## GitHub Actions

`type = "github_actions"` targets repositories released as a git tag plus a provider release. GitHub Actions can be consumed as npm packages, Docker images, composites, or plain scripts, so there is no registry publish step: the tag and the GitHub release are the publish.

Example configuration:

```toml
[package.actions]
path = "."
type = "github_actions"
version_format = "primary"

[source]
provider = "github"
owner = "acme"
repo = "actions"

[source.releases]
enabled = true
source = "monochange"
```

GitHub Actions behavior:

- the type preset implies `version_source = "tag"`, `tag = true`, `release = true`, `publish.enabled = false`, and `initial_version = "0.1.0"`; every field can be overridden explicitly
- discovery reads the package directory and synthesizes the release identity from `action.yml` (or `action.yaml`); the manifest itself carries no version
- a sibling `package.json` `version` field is synced to the released version when present; set `ignore_ecosystem_versioned_files = true` or declare explicit `versioned_files` to control this
- release tags follow `version_format`; multi-action repos should keep the default `namespaced` format so tags look like `my-action/v1.2.3`
- floating tag aliases (`floating_tags`) move to each release, reproducing the common `v1.2` / `v1` moving tags

## Choosing external publishing

Use `mode = "external"` when an ecosystem or registry is not handled by monochange's built-in publisher, or when your organization needs custom signing, provenance, approval, rate-limit, private-registry behavior, a Python publishing toolchain other than the built-in `uv build` / `uv publish` flow, or a Go publishing workflow that signs, pushes, or annotates tags outside monochange.

That keeps the package in release planning while leaving upload mechanics to your existing publishing workflow.
