# Trusted publishing and OIDC

monochange supports built-in package publishing for the canonical public registries of the ecosystems it manages:

- Cargo → `crates.io`
- npm packages → `npm`
- Deno packages → `jsr`
- Dart / Flutter packages → `pub.dev`
- Python packages → `pypi`
- Go modules → `go_proxy` via VCS tags

For those registries, monochange can also manage or verify **trusted publishing** when the registry supports publishing directly from a verified GitHub Actions identity. PyPI is supported by the built-in publisher through `uv build` and `uv publish`; PyPI trusted-publisher enrollment is still completed manually in the PyPI project settings.

Different registries use different names for the same general pattern:

- **trusted publishing**
- **OIDC publishing**
- **automated publishing**
- **trusted publishers**

The goal is the same in every case:

- publish from CI instead of local machines
- avoid long-lived registry tokens where possible
- restrict publish rights to a specific repository, workflow, and sometimes environment

## Provider and registry capability matrix

Trusted publishing support is not uniform across registries or CI providers. monochange models these dimensions separately so it can be strict where support is verifiable and honest where setup still needs manual review.

| Ecosystem      | Registry  | Trusted-publishing providers modeled by monochange | Current CI identity can be detected | Publish-time setup/context can be verified by monochange | Registry-side setup can be automated by monochange    | Registry-native provenance / attestations |
| -------------- | --------- | -------------------------------------------------- | ----------------------------------- | -------------------------------------------------------- | ----------------------------------------------------- | ----------------------------------------- |
| npm            | npm       | GitHub Actions, GitLab CI/CD                       | Yes                                 | GitHub Actions context only                              | No; run `npm trust github ...` manually or externally | Yes, npm provenance                       |
| cargo          | crates.io | GitHub Actions                                     | Yes                                 | No                                                       | No                                                    | No registry-native package provenance     |
| deno           | jsr       | GitHub Actions                                     | Yes                                 | No                                                       | No                                                    | Yes, JSR package provenance               |
| dart / flutter | pub.dev   | GitHub Actions, Google Cloud Build                 | Yes                                 | No                                                       | No                                                    | No registry-native package provenance     |
| python         | PyPI      | GitHub Actions, GitLab CI/CD, Google Cloud Build   | Yes                                 | No                                                       | No                                                    | Yes, PEP 740 digital attestations         |
| go             | Go proxy  | None; VCS tags are used instead                    | N/A                                 | N/A                                                      | Creates module tags                                   | Source-control provenance only            |
| custom/private | custom    | None by default                                    | Provider may be detected            | No                                                       | No                                                    | Unknown                                   |

monochange also detects CircleCI publish-time identity, but none of the built-in public registry combinations above are treated as CircleCI trusted-publishing support today. Unknown local shells and unsupported providers are never treated as trusted.

npm is the only ecosystem where monochange can render precise GitHub trusted-publisher repair commands. It still does not run `npm trust` during real package publishing; use `mode = "external"` for any registry workflow that should stay outside monochange's built-in publisher.

Go module publishing is included in the built-in package publisher, but it is not an OIDC trusted-publishing flow. Go versions are published by creating VCS tags. monochange uses `git tag`, choosing `v1.2.3` for a root module and path-prefixed tags such as `api/v1.2.3` for submodules, then checks availability through the Go module proxy.

For `crates.io`, `jsr`, `pub.dev`, and PyPI, monochange reports the setup URL for each package and blocks the next built-in registry publish until the trust configuration has been completed manually. It also preflights the trusted-publishing context for those registries, surfacing the provider capability message and, for GitHub contexts, the repository, workflow, and environment it expects when that context can be resolved.

## monochange configuration

Start by enabling trusted publishing for the relevant ecosystem. Packages inherit the ecosystem publish setting by default and can override it when needed.

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases.attestations]
require_github_artifact_attestations = true

[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.trusted_publishing]
workflow = "publish.yml"
environment = "publisher"

[ecosystems.npm.publish.attestations]
require_registry_provenance = true

[ecosystems.cargo.publish]
trusted_publishing = true

[ecosystems.deno.publish]
trusted_publishing = true

[ecosystems.deno.publish.attestations]
require_registry_provenance = true

[ecosystems.dart.publish]
trusted_publishing = true

[ecosystems.python.publish]
trusted_publishing = true

[package.cli.publish.trusted_publishing]
workflow = "publish-cli.yml"

[package.legacy.publish]
trusted_publishing = false
```

Use ecosystem publish settings for the shared trust policy and GitHub context. Use package publish settings only for package-specific workflows, environments, or opt-outs.

monochange resolves the GitHub trust context from:

- `publish.trusted_publishing.repository`
- `publish.trusted_publishing.workflow`
- `publish.trusted_publishing.environment`
- otherwise `[source]`
- otherwise GitHub Actions runtime values such as `GITHUB_REPOSITORY`, `GITHUB_WORKFLOW_REF`, and `GITHUB_JOB`

If your workflow filename or environment cannot be inferred reliably, set them explicitly in `monochange.toml`.

### Requiring or preferring trusted publishing

`publish.trusted_publishing.mode` decides what happens when no verifiable CI/OIDC identity is available:

- `mode = "required"` (default). Publishing must run from a verifiable CI identity. Local and manual runs fail before any registry mutation. Use this when trusted publishing is the only publishing path you want to allow.
- `mode = "preferred"`. Trusted publishing is used whenever a verifiable CI identity is detected, and publishing falls back to local/manual credentials otherwise. Detected CI identities are still verified against the configured repository, workflow, and environment, so a misconfigured CI context fails instead of silently publishing with a token.

```toml
[ecosystems.dart.publish.trusted_publishing]
enabled = true
mode = "preferred"
repository = "acme/widgets"
workflow = "publish.yml"
environment = "publisher"
```

`preferred` fits repositories that publish from CI with OIDC trusted publishing but also let maintainers run `monochange run publish` locally with their own registry credentials. The mode only relaxes the identity requirement. It never disables the CI context verification, and `enabled = false` remains the explicit opt-out from trusted publishing entirely.

## Staged publishing on npm

Trusted publishing and staged publishing answer different questions, and npm packages can use both at once:

- **Trusted publishing** authenticates _who_ is publishing. The registry exchanges a short-lived OIDC token from a specific repository/workflow/environment for publish credentials, so there is no stored token to steal. On its own it says nothing about _when_ a version goes live: a direct `npm publish` from a verified workflow is installable immediately.
- **Staged publishing** controls _when_ a version goes live. `npm stage publish` uploads the version to a staging queue; it becomes installable only after a maintainer approves it with 2FA (`npm stage approve <stage-id>` in the CLI, or the Staged Packages tab on npmjs.com). The staging step needs no 2FA, so CI can run unattended.
- **Direct publishing** (`npm publish`) is the default: the version is installable the moment the command succeeds, authenticated by a login session, a token, or a trusted-publishing OIDC exchange.

The strongest posture combines them: a trusted-publishing workflow stages token-free, and a human authorizes release with 2FA. Approval always requires an interactive 2FA challenge — it cannot be automated with OIDC or CI tokens — which is exactly why it survives a compromised CI context. A hijacked workflow can stage a malicious version, but it cannot approve it. New npm trusted-publisher connections created after September 2026 default to allowing only `npm stage publish`, and npm removes direct publishing with 2FA-bypass tokens in January 2027, so staged publishing is also the migration path for token-based automation.

Configure the release flow per ecosystem or package with `publish.flow`:

```toml
[ecosystems.npm.publish]
trusted_publishing = true
flow = "staged"

[package.legacy.publish]
# Keep immediate releases for this one package.
flow = "direct"
```

Behavior details:

- Release publishes run `npm stage publish --access public` (pnpm workspaces run `pnpm stage publish`); `--provenance` is still passed when attestations require it.
- A successful staged publish reports a `staged` status, not `published`. The version is **not** installable yet, so approve it before announcing the release or pointing consumers at the new version.
- Approve staged versions with `npm stage list`, then `npm stage approve <stage-id>` (or the Staged Packages tab on npmjs.com). monochange deliberately does not automate approval because it requires interactive 2FA.
- Placeholder publishing always stays direct: a placeholder's purpose is to register the package immediately so trusted publishing can be configured.
- The registry version probe cannot see staged-but-unapproved versions. A publish re-run before approval stages again rather than skipping, and publish resume treats staged outcomes as complete for the packages that already staged successfully.
- Staged publishing requires npm CLI 11.15+ and Node 22.14+ (pnpm 11.3+ for pnpm workspaces). With token-based automation, prefer a stage-only granular access token, which can stage and move dist-tags but can never publish directly.

## Attestation and provenance policy

Trusted publishing and attestations answer different questions:

- **trusted publishing** decides which CI/OIDC identity may publish a package
- **registry-native package provenance** records where a published package came from in registries that support it, such as npm provenance, JSR provenance, and PyPI PEP 740 attestations
- **GitHub release artifact attestations** cover release assets uploaded to GitHub Releases and are separate from package-registry provenance

Trusted publishing does not automatically require registry provenance. Enable provenance explicitly for registries where you want monochange to enforce it:

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.attestations]
require_registry_provenance = true
```

Packages inherit `publish.attestations` from their ecosystem publish settings. Package-level settings can override or opt out:

```toml
[package.legacy.publish.attestations]
require_registry_provenance = false
```

When `publish.attestations.require_registry_provenance = true`, built-in release publishing fails before invoking the registry command unless all of these are true:

1. `publish.trusted_publishing = true` is effective for the package.
2. The current environment exposes a verifiable CI/OIDC identity.
3. The provider/registry capability matrix says registry-native provenance is available for that identity.

Today this is enforceable for npm trusted publishing and JSR publishing from supported OIDC contexts. The capability matrix records PyPI PEP 740 attestations as registry-native provenance, but monochange rejects `require_registry_provenance` for PyPI until the built-in Python publisher exposes a publish command that can require uploading those attestations. It also rejects `crates.io`, `pub.dev`, Go proxy publication, and custom registries because those flows do not expose registry-native package provenance that monochange can require without creating false assurance.

For GitHub release assets, keep the policy under `[source.releases.attestations]`:

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[source.releases.attestations]
require_github_artifact_attestations = true
```

This setting is accepted only for the GitHub source provider. It records that release assets are expected to be covered by GitHub Artifact Attestations; the workflow that builds/uploads assets must still grant `attestations: write` and run the GitHub attestation action for the uploaded subjects.

## GitHub Actions baseline

Most registries require the publish job to request an OIDC token.

```yaml
permissions:
  contents: read
  id-token: write
```

If you use a protected deployment environment, keep the workflow and registry settings aligned:

```yaml
jobs:
  publish:
    environment: publisher
```

Use the same environment name in GitHub Actions and in the registry configuration.

When `publish.trusted_publishing = true`, release publishing is a mandatory CI/OIDC flow. Built-in publish commands reject local/manual execution before invoking registry CLIs, require the configured GitHub repository/workflow/environment to match the current job, require `id-token: write`, and refuse long-lived npm token environment variables such as `NPM_TOKEN` or `NODE_AUTH_TOKEN`. Use `publish.trusted_publishing = false` only for packages that intentionally opt out.

## Recommended rollout

Use this sequence when adopting trusted publishing for an existing workspace:

1. Set `publish.trusted_publishing = true` for the target ecosystem, then override individual packages only when they differ.
2. Run `monochange publish placeholder --dry-run` to see which packages do not exist yet.
3. If needed, run `monochange publish placeholder` so the package exists in the registry first.
4. Complete the registry-side trusted-publishing setup for each package.
5. Run `monochange publish packages --dry-run` to confirm monochange sees the expected trust configuration.
6. Optionally generate a readiness artifact in CI with `monochange publish readiness --from HEAD --output .monochange/readiness.json` for preflight review or publish planning.
7. Publish from CI with `monochange publish packages --output .monochange/publish-result.json`.

Placeholder publishing is especially useful when the package name must exist in the registry before trusted publishing can be configured.

## npm

### Registry-side setup

On npm, trusted publishing can be configured from the package settings page or through the CLI.

**UI path**

- `npmjs.com` → package → **Settings** → **Trusted publishing**

**Fields to enter for GitHub Actions**

- **Organization or user**: GitHub owner
- **Repository**: GitHub repository name
- **Workflow filename**: for example `publish.yml`
- **Environment name**: optional, for example `publisher`

Only the workflow filename is required, not the full `.github/workflows/...` path.

### CLI setup commands

These are the same commands monochange models for npm trusted publishing.

List the current trusted publishers for a package:

```bash
npm trust list <package-name> --json
```

Configure a package for a GitHub workflow:

```bash
npm trust github <package-name> \
  --repo owner/repo \
  --file publish.yml \
  --yes
```

Add an environment restriction:

```bash
npm trust github <package-name> \
  --repo owner/repo \
  --file publish.yml \
  --env publisher \
  --yes
```

If the workspace uses pnpm, use the pnpm-wrapped form:

```bash
pnpm exec npm trust github <package-name> \
  --repo owner/repo \
  --file publish.yml \
  --env publisher \
  --yes
```

### Workflow requirements

At minimum, the publish workflow should have:

```yaml
permissions:
  contents: read
  id-token: write
```

### monochange notes

- Real package publishing does not execute `npm trust` or `npm trust list` automatically.
- monochange verifies the configured GitHub Actions OIDC context before trusted npm publishing.
- If trust needs repair, monochange reports the exact `npm trust github ...` command to run manually or from separate tooling.
- pnpm workspaces publish with `pnpm publish`, including under trusted publishing, so `workspace:` dependency ranges are rewritten to real versions in the published tarball. Other npm-family workspaces publish with the `npm` CLI. Manual trust setup in a pnpm workspace still uses `pnpm exec npm trust ...`.
- Trusted publishing from a pnpm workspace requires pnpm 11.1.3 or later. pnpm 11 performs the npm OIDC exchange natively; pnpm 10 delegates to the npm CLI and does not reliably complete it. Pin the publish job's pnpm, for example with `pnpm/action-setup` and `version: 11`.
- pnpm publishes run with `--no-git-checks`: monochange validates the release commit itself, and release jobs usually run from a detached tag checkout that pnpm's branch and clean-tree checks would reject.

## crates.io

### Registry-side setup

crates.io supports trusted publishing through CI-issued OIDC. monochange models GitHub Actions for built-in trusted-publishing diagnostics and keeps other crates.io provider combinations manual until registry-side verification is available.

Trusted publishing on crates.io exchanges your CI identity for a short-lived publish token, so you do not need a long-lived crates.io API token in CI.

**Prerequisites**

- the crate must already exist on `crates.io`
- you must be an owner of the crate on `crates.io`
- the repository must live on GitHub or GitLab

If the crate does not exist yet, bootstrap it first with a real initial release or `monochange publish placeholder`. The first publish still uses the normal crates.io token flow.

**UI path**

- crate page → **Settings** → **Trusted Publishing**

**Fields to enter for GitHub Actions**

- **Repository owner**: GitHub owner
- **Repository name**: GitHub repository name
- **Workflow filename**: for example `release.yml`
- **Environment**: optional, for example `release`

Use the workflow filename only, not the full `.github/workflows/...` path.

### Workflow setup

A typical GitHub Actions release job looks like this:

```yaml
name: Publish to crates.io

on:
  push:
    tags:
      - "v*"

jobs:
  publish:
    runs-on: ubuntu-latest
    environment: release
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
        id: auth
      - run: cargo publish
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
```

If you configure an environment on crates.io, the GitHub job must use the same environment name.

### monochange notes

- monochange does not create the `crates.io` trusted-publisher record for you yet.
- monochange preflights the GitHub repository/workflow/environment context it expects for manual registries and reports when one of those values still needs to be set explicitly in config.
- Once the registry-side configuration exists, monochange can publish with the temporary token exposed by `rust-lang/crates-io-auth-action@v1`.
- crates.io issues a short-lived publish token; the current docs describe these tokens as expiring after 30 minutes.
- Use a specific workflow filename and, when needed, a protected GitHub environment to reduce the publish attack surface.
- The current monochange GitHub publish workflow already uses this pattern.

Useful references:

- `https://crates.io/docs/trusted-publishing`
- `https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html`

## jsr

### Registry-side setup

JSR supports tokenless publishing from GitHub Actions.

**Manual setup step**

- go to the package on `jsr.io`
- open **Settings**
- link the package to the GitHub repository that is allowed to publish it

monochange reports the package URL and expects this repository-linking step to be completed manually.

### Workflow setup

A minimal GitHub Actions job looks like this:

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: actions/checkout@v6
  - run: npx jsr publish
```

You can also publish with:

```bash
deno publish
```

### monochange notes

- JSR's tokenless publishing is GitHub Actions focused.
- Other CI providers still need token-based publishing.
- monochange does not yet link the package to the repository for you.
- If the package does not exist yet, placeholder publishing can bootstrap the registry entry before you finish the repository-link step.

Useful references:

- `https://jsr.io/docs/publishing-packages`
- `https://jsr.io/docs/trust`

## pub.dev

### Registry-side setup

pub.dev calls this **automated publishing**.

Automated publishing on pub.dev authenticates with a temporary GitHub-signed OIDC token instead of a long-lived pub credential.

**Prerequisites**

- the package must already exist on `pub.dev`
- you must be an uploader or admin for the package
- the repository must be on GitHub

If the package does not exist yet, publish it once first or use `monochange publish placeholder`.

**UI path**

- `https://pub.dev/packages/<package>/admin`
- find the **Automated publishing** section
- click **Enable publishing from GitHub Actions**

**Fields to enter**

- **Repository**: `owner/repo`
- **Tag pattern**: the package's monochange release tag with `{{version}}` in place of the version

Examples:

- package released under the primary tag (`version_format = "primary"`): `v{{version}}`
- package with its own tag (`tag = true` and the default `namespaced` format): `my_package/v{{version}}`
- package with a custom `version_format`, such as `"{{ name }}-v{{ version }}"`: `my_package-v{{version}}`

For a monorepo, give each package its own tag pattern so a tag for one package cannot publish another package by accident. For a broader monorepo strategy across registries, see [Multi-package publishing patterns](./14-multi-package-publishing.md).

**Workflow dispatch**

- tick **Enable publishing from `workflow_dispatch` events** so monochange can publish a package by dispatching the publish workflow on the package's tag (see [How monochange publishes to pub.dev](#how-monochange-publishes-to-pubdev))
- keep **Enable publishing from `push` events** ticked if you also publish from a workflow that tag pushes start

**Optional hardening**

- click **Require GitHub Actions environment** on the package admin page
- choose an environment name such as `pub.dev`
- use the same environment name in the GitHub workflow

### Workflow requirements

pub.dev checks the GitHub OIDC token of the run that uploads the package. It accepts the upload only when all of these hold:

- the run's event is `push` or `workflow_dispatch`, and that event is enabled on the package admin page
- the run's ref is a tag (`ref_type` is `tag`), not a branch
- the run's ref is exactly `refs/tags/` followed by the tag pattern with `{{version}}` replaced by the version being published; for the pattern `my_package/v{{version}}` and version `0.2.1`, that is `refs/tags/my_package/v0.2.1`
- the run uses the required GitHub Actions environment, when the package requires one

So a run on one tag can only publish the pub.dev packages whose tag pattern renders to that tag. A run on the primary `v1.4.0` tag can never publish a package versioned `0.2.1` under `my_package/v{{version}}`.

### How monochange publishes to pub.dev

monochange applies the pub.dev rule before it uploads anything. A Dart or Flutter package is **tag-bound** when monochange publishes it to pub.dev with trusted publishing: the built-in publisher, `publish.trusted_publishing` on (the default), and no `PUB_TOKEN` in the environment. An empty `PUB_TOKEN`, which GitHub Actions passes for an unset secret, does not count. For every GitHub Actions run, monochange decides which packages the run publishes:

- **A run on a tag-bound package's own tag**, when that tag is not the release's primary tag (for example `v1.4.0`), publishes only the tag-bound packages released under that tag. Every other package is marked `skipped_out_of_scope`. That is not a failure, monochange does not look the package up on its registry, and the package's report line names the run that publishes it.
- **Every other run** (on the primary tag, on a branch, or on a tag that is no tag-bound package's tag) publishes every package that is not tag-bound, as monochange always has. A tag-bound package whose tag is the run's tag publishes too, so a pub.dev package that owns the primary tag publishes beside every other package. Any other tag-bound package is marked `deferred_to_tag`.

A deferred package is not uploaded. The run does not fail, and the packages after it still publish. The report lists the tag the package needs and the command that publishes it, such as `gh workflow run publish.yml --repo owner/repo --ref my_package/v0.2.1`. The command dispatches the workflow set in `publish.trusted_publishing.workflow`, or the current workflow when the package sets none. With `--format json`, the package row carries a `deferred_to` object with the `tag`, `ref`, `command`, and a `reason`:

| `reason`         | The run                                                                                         | What to change                                                                                           |
| ---------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `wrong_ref`      | is on another tag                                                                               | run the command, which dispatches the workflow on the package's tag                                      |
| `wrong_ref_type` | is on a branch or a pull request ref                                                            | run the command, which dispatches the workflow on the package's tag                                      |
| `wrong_event`    | is on the package's tag, but an event pub.dev rejects started it, such as `release` or `create` | start the workflow from a tag push (`on: push`) or with `workflow_dispatch`; the command does the latter |
| `unknown_ref`    | has no `GITHUB_REF`, so monochange cannot tell its ref                                          | keep GitHub's `GITHUB_REF` in the publish step's environment, or run the command                         |

When a release creates no tag besides tag-bound packages' tags, the run on such a tag is the only tag run the release gets, so it publishes every package, as before.

A package can only publish from its own tag when monochange creates one. `monochange step validate` and `monochange publish readiness` report an error for a trusted pub.dev package whose releases create no tag, and name the section to fix: set `tag = true` on the package, or on its group when the package belongs to one. `monochange step validate` reads only the configuration, so a package that publishes with a `PUB_TOKEN` needs `publish.trusted_publishing = false` to pass it. Packages that set `publish.trusted_publishing = false` or `publish.mode = "external"`, and private packages (`publish_to: none`), are not checked.

`monochange publish readiness` checks every package of the release, whatever ref the run that checks it is on.

Outside GitHub Actions, monochange cannot read the run's ref, so it does not apply this rule. monochange treats a run as GitHub Actions when `GITHUB_ACTIONS=true` or `GITHUB_WORKFLOW_REF` is set, the same check it uses to detect the trusted-publishing identity.

### Recommended reusable workflow

pub.dev strongly encourages the reusable workflow maintained by `dart-lang/setup-dart`:

```yaml
name: Publish to pub.dev

on:
  push:
    tags:
      - "v[0-9]+.[0-9]+.[0-9]+"

jobs:
  publish:
    permissions:
      id-token: write
    uses: dart-lang/setup-dart/.github/workflows/publish.yml@v1
    # with:
    #   working-directory: path/to/package/within/repository
```

If you require a GitHub Actions environment on pub.dev, pass the same environment name to the reusable workflow:

```yaml
jobs:
  publish:
    permissions:
      id-token: write
    uses: dart-lang/setup-dart/.github/workflows/publish.yml@v1
    with:
      environment: pub.dev
      # working-directory: path/to/package/within/repository
```

### Custom workflow

If you need custom code generation or build steps, set up Dart yourself and publish manually after the OIDC-authenticated setup step:

```bash
dart pub publish --force
```

For Flutter packages, the equivalent publish command is:

```bash
flutter pub publish --force
```

### monochange notes

- monochange does not configure pub.dev automated publishing for you yet.
- monochange reports the package admin URL so you can finish the setup manually.
- pub.dev is stricter than the other registries because the run must be on the package's own tag. monochange defers a package to its tag instead of failing the run; see [How monochange publishes to pub.dev](#how-monochange-publishes-to-pubdev).
- The reusable workflow from `dart-lang/setup-dart` is the officially recommended path and is worth preferring unless you need custom pre-publish steps.
- Keep the Git tag, `pubspec.yaml` version, and tag pattern aligned.
- Protect matching tags, and use GitHub environment protection rules when you need an approval gate before publishing.

Useful references:

- `https://dart.dev/tools/pub/automated-publishing`
- `https://pub.dev/packages/<package>/admin`

## Mapping monochange config to registry values

Use this cheat sheet when a registry asks for workflow details.

| Registry field                              | Value to use                                                                                          |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| repository owner / organization / namespace | GitHub owner from `[source]` or `publish.trusted_publishing.repository`                               |
| repository name / project                   | repository part of `owner/repo`                                                                       |
| workflow filename                           | `publish.trusted_publishing.workflow`, for example `publish.yml`                                      |
| environment                                 | `publish.trusted_publishing.environment`, for example `publisher`                                     |
| pub.dev tag pattern                         | the package's release tag with `{{version}}`, for example `v{{version}}` or `my_package/v{{version}}` |

If monochange cannot infer the GitHub repository or workflow for a package, set them explicitly in `monochange.toml`.

## Security recommendations

- prefer trusted publishing over long-lived registry tokens whenever the registry supports it
- keep `id-token: write` only on the publish job instead of the entire workflow when possible
- use a protected GitHub environment such as `publisher` for high-value publish jobs
- restrict tag creation and release workflows to trusted maintainers
- use package-specific tags in monorepos when a registry supports tag-based publish authorization

## When to keep `mode = "external"`

Keep a package on `mode = "external"` when:

- the registry is private or custom
- you need retry or delayed requeue behavior that monochange does not manage yet
- your registry requires a CI pattern that differs substantially from monochange's built-in publish flow

In those cases, you can still use the same registry-side trusted-publishing setup while letting your own workflow own the actual publish command. The same approach is often the cleanest fit for multi-package repositories that need package-specific tags or workflows; see [Multi-package publishing patterns](./14-multi-package-publishing.md).

## Possible future automation for manual registries

monochange is intentionally conservative here.

Today, monochange does not perform registry-side trusted-publishing enrollment during real package publishing. npm has the richest repair guidance because monochange can render the expected `npm trust github ...` command, but the command is still manual or external. For `crates.io`, `jsr`, `pub.dev`, and PyPI, monochange focuses on setup guidance, preflight checks, and actionable diagnostics instead of trying to mutate registry-side trust records automatically.

Areas that may become more automated later, where the registry and CI contracts make it safe enough, include:

- **`crates.io`**: stronger preflight validation around explicit workflow filenames, environment alignment, and clearer checks for first-publish bootstrap versus post-bootstrap trusted publishing
- **`jsr`**: better repository-link diagnostics and package metadata checks before publish, especially when the package already exists but repository-side linking is incomplete
- **`pub.dev`**: stronger validation that tag patterns, workflow triggers, working directories, and optional environments still match the automated-publishing setup expected by pub.dev
- **PyPI**: stronger validation that the project trusted-publisher settings match the workflow name, environment, and package path monochange expects before running `uv publish`

Areas that monochange does **not** promise:

- auto-enrolling registry-side trusted-publisher records for any registry, including npm, `crates.io`, `jsr`, `pub.dev`, or PyPI
- bypassing browser-confirmed or admin-page-only steps that the registry intentionally keeps manual
- inferring enough registry-side state to claim a package is fully enrolled when the registry does not expose that state safely or consistently

Treat this as a direction of travel, not a guarantee of upcoming behavior. If you need a registry-native workflow, keep the package on `mode = "external"` and let the registry-maintained workflow own the actual publish command.
