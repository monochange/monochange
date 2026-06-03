# Advanced: CI, package publishing, and release PR flows

This guide brings together the practical CI patterns around `monochange step publish-packages`, `monochange step placeholder-publish`, `monochange step open-release-request`, `monochange step commit-release`, and provider release automation.

It also documents the recommended workflow for long-running release PR branches.

## Start with the command surface

These commands solve different automation problems:

<!-- {=projectCommandAutomationMatrix} -->

These are common commands for repositories using monochange. With the current CLI model, workflow names such as `discover`, `change`, `release`, `publish`, and `affected` come from optional `[cli.*]` tables in `monochange.toml`; binary commands such as `check`, `init`, `sync`, and `mcp` stay built in, while typed built-in operations such as validation are exposed as immutable `monochange step *` commands.

| Goal                             | Command                                                                              | Use it when                                                                                                                        |
| -------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| Validate config and changesets   | `monochange step validate`                                                           | You changed `monochange.toml` or `.changeset/*.md` files                                                                           |
| Inspect package ids and groups   | `monochange step discover --format json`                                             | You need the normalized workspace model                                                                                            |
| Sync internal dependency ranges  | `monochange sync versions --dry-run`                                                 | You want Dart or npm internal dependency references to match canonical workspace package versions                                  |
| Create release intent            | `monochange change --package <id> --bump <severity> --reason "..."`                  | You need a new `.changeset/*.md` file                                                                                              |
| Audit pending release context    | `monochange step diagnose-changesets --format json`                                  | You need git provenance, PR/MR links, or related issues                                                                            |
| Preview the release plan         | `monochange release --dry-run --diff` or `monochange step prepare-release --dry-run` | You want changelog/version patches without mutating the repo                                                                       |
| Create a durable release commit  | `monochange step commit-release`                                                     | You want a monochange-managed release commit with an embedded `ReleaseRecord`                                                      |
| Open or update a release request | `monochange step open-release-request`                                               | You want a long-lived release PR/MR branch updated from current release state                                                      |
| Inspect a past release commit    | `monochange step release-record --from <ref>`                                        | You need the durable release declaration from git history                                                                          |
| Check package publish readiness  | `monochange step publish-readiness --from HEAD --output <path>`                      | You want a non-mutating preflight report before package publication                                                                |
| Dry-run configured publishing    | `monochange publish-check`                                                           | This repository, or another repo with a similar `[cli.publish-check]`, should exercise publishing in CI without registry mutations |
| Plan ready package publishing    | `monochange step plan-publish-rate-limits --readiness <path>`                        | You want rate-limit batches that exclude non-ready package work                                                                    |
| Publish packages to registries   | `monochange step publish-packages --output <path>`                                   | You want `cargo publish`, `npm publish`, `deno publish`, or `dart pub publish` style package publication                           |
| Bootstrap release packages       | `monochange step placeholder-publish --from HEAD --output <path>`                    | You need a release-record-scoped placeholder bootstrap artifact before rerunning readiness                                         |
| Create post-merge release tags   | `monochange step tag-release --from HEAD`                                            | You merged a monochange release commit and now need to create and push its declared tag set                                        |
| Repair a recent release          | `monochange step retarget-release --from <tag> --target <commit>`                    | You need to retarget a just-created release to a later commit                                                                      |
| Publish hosted/provider releases | `monochange step publish-release`                                                    | You want GitHub/GitLab/Gitea release objects from prepared release state                                                           |

<!-- {/projectCommandAutomationMatrix} -->

A practical rule of thumb:

- use **`monochange step publish-readiness`** for registry preflight reports and **`monochange step publish-packages`** for registry package publication
- use **`monochange step publish-release`** for hosted releases from prepared release state
- use **`monochange step open-release-request`** when you want a provider-backed release request branch
- use **`monochange step commit-release`** when you want a durable local release commit in git history
- use **`monochange step tag-release`** when that durable release commit has merged and you want to create its tag set on the default branch

## The three automation layers

monochange has three related but different automation layers:

1. **Release planning** — `monochange release --dry-run`, `monochange release`, `monochange step diagnose-changesets`
2. **Package registries** — `monochange step publish-readiness`, `monochange step placeholder-publish --from HEAD`, `monochange step plan-publish-rate-limits --readiness <path>`, `monochange step publish-packages`, and lower-level `monochange step placeholder-publish`
3. **Hosted providers** — `monochange step open-release-request`, `monochange step publish-release`, `monochange step retarget-release`

Keeping those layers separate is important. Package publication and hosted-release publication are not the same job.

## Registry and provider capability snapshot

<!-- {=projectCapabilityMatrix} -->

| Capability                                                                     | Current status                                                                                                                         |
| ------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| Multi-ecosystem discovery                                                      | Cargo, npm/pnpm/Bun, Deno, Dart, Flutter, Python, Go                                                                                   |
| Package release planning                                                       | Built in                                                                                                                               |
| Grouped/shared versioning                                                      | Built in                                                                                                                               |
| Internal dependency version synchronization                                    | Dart and npm via `monochange sync versions`; release planning still updates supported ecosystems during releases                       |
| Dry-run release diff previews                                                  | Built in via `monochange step prepare-release --dry-run --diff`; configured workflows may expose `monochange release --dry-run --diff` |
| Durable release history and post-merge tagging                                 | Built in via `ReleaseRecord`, `monochange step release-record`, `monochange step tag-release`, and `monochange step retarget-release`  |
| Hosted provider releases                                                       | GitHub, GitLab, Gitea, Forgejo                                                                                                         |
| Hosted release requests                                                        | GitHub, GitLab, Gitea, Forgejo                                                                                                         |
| Python release planning                                                        | Built in for discovery, version rewrites, dependency rewrites, lockfile command inference, and PyPI publishing                         |
| Go release planning                                                            | Built in for `go.mod` discovery, dependency rewrites, `go mod tidy` inference, and Go proxy tag publishing                             |
| Built-in registry publishing                                                   | `crates.io`, `npm`, `jsr`, `pub.dev`, `pypi`, Go proxy tags; use external mode for custom registries                                   |
| GitHub npm trusted-publishing diagnostics                                      | Built in; registry-side enrollment stays manual or external                                                                            |
| GitHub trusted-publishing guidance for `crates.io`, `jsr`, `pub.dev`, and PyPI | Built in, but manual registry enrollment is still required                                                                             |
| GitLab trusted-publishing auto-derivation                                      | Not built in today                                                                                                                     |
| Release-retarget sync for hosted releases                                      | GitHub first                                                                                                                           |

<!-- {/projectCapabilityMatrix} -->

## CI setup assumption

The workflow sketches below assume the job already has:

- the `monochange` CLI available as `monochange`
- the native ecosystem toolchain it needs (`npm`/`pnpm`, `cargo`, `deno`, `dart`, `flutter`, `uv`, `poetry`, or your external publishing tool)
- repository checkout with enough history for release-record inspection

In the monochange repository itself, that usually means entering the `devenv` shell. In other repositories, it may mean installing `@monochange/cli` or `monochange` explicitly before the publish step.

## GitHub flows

### Common GitHub shape

For GitHub Actions, the most common structure is:

1. a workflow prepares or updates a release PR branch
2. a release commit lands on `main`
3. a post-merge workflow detects the release commit
4. that workflow creates the declared tags and publishes packages from the durable release commit
5. hosted release objects or extra assets come either from downstream tag-driven workflows or from a separate workflow that still uses `monochange step publish-release`

The important current implementation detail is that `monochange step publish-readiness` can write a preflight artifact from the `ReleaseRecord` on `HEAD`, `monochange step placeholder-publish --from HEAD --output <path>` can run release-record-scoped first-time placeholder setup and record the result, `monochange step publish-packages` publishes directly from prepared release or `HEAD` release state, `monochange step tag-release` can create the declared release tags from that same durable record, and `monochange step publish-release` still works from prepared release state when you want a manifest-driven hosted-release job. The readiness artifact also fingerprints publish inputs that affect registry behavior for planning: `monochange.toml`, package manifests, lockfiles, and registry/tooling files such as `.npmrc`, `.cargo/config.toml`, `rust-toolchain.toml`, workspace `Cargo.toml`, and ecosystem manifests.

If the same post-merge job is responsible for both tags and package publication, run `monochange step tag-release --from HEAD` immediately after release-commit detection, then run `monochange step publish-readiness --from HEAD --output <path>`, use `monochange step placeholder-publish --from HEAD --output <path>` only when first-time package setup is required, optionally inspect `monochange step plan-publish-rate-limits --readiness <path>`, and finally run `monochange step publish-packages --output .monochange/publish-result.json`. Rerun `monochange step publish-readiness` if CI setup edits publish inputs after the artifact is written. If a registry command fails after some packages were published, fix the cause and rerun `monochange step publish-packages --resume .monochange/publish-result.json --output .monochange/publish-result.json`; monochange skips completed package versions from the previous result and retries the remaining release work.

### Tag-release JSON for follow-up workflows

<!-- {=projectTagReleaseJsonTagsMap} -->

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

### GitHub + npm trusted publishing

Config:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.npm.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
```

Workflow sketch:

```yaml
name: publish-npm

on:
  push:
    branches: [main]

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - name: checkout
        uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - name: setup repo tooling
        uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: detect monochange release commit
        shell: bash
        run: |
          set -euo pipefail
          if ! devenv shell -- monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "HEAD is not a monochange release commit; skipping publish"
            exit 0
          fi

      - name: publish npm packages
        run: |
          devenv shell -- monochange step publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- monochange publish
```

What monochange does here:

- resolves the GitHub workflow context
- rejects long-lived npm token environment variables when trusted publishing is enabled
- verifies that the publish job is running from the configured GitHub Actions OIDC context
- reports the `npm trust github ...` repair command when setup needs manual or external repair
- publishes trusted npm packages with the `npm` CLI directly

Run `npm trust github ...` separately before this workflow if npm has not been enrolled yet; `monochange step publish-packages` does not execute `npm trust` during real publishing.

### GitHub + Cargo (`crates.io`) trusted publishing

Config for monochange-managed release planning:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.cargo.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
```

monochange-oriented post-merge workflow sketch:

```yaml
name: publish-cargo

on:
  push:
    branches: [main]

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: detect monochange release commit
        shell: bash
        run: |
          set -euo pipefail
          if ! devenv shell -- monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "HEAD is not a monochange release commit; skipping publish"
            exit 0
          fi

      - name: publish Cargo packages
        run: |
          devenv shell -- monochange step publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- monochange publish
```

More copy-pasteable registry-native example:

If you want to follow the crates.io documentation more literally, let the official auth action own the token exchange and keep monochange focused on release planning. In that case, prefer `mode = "external"` for Cargo publication.

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.cargo.publish]
enabled = true
mode = "external"
trusted_publishing = true
```

```yaml
name: publish-cargo

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
      - run: cargo publish --package my_crate
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
```

For monorepos with multiple Cargo packages, split this into one job per published crate or have an external script decide which crates should publish for the current tag. For a broader decision guide across built-in and external multi-package flows, see [Multi-package publishing patterns](./14-multi-package-publishing.md).

Important current behavior:

- monochange can carry the trust expectation in config
- monochange can report the setup URL and enforce that trust is configured before built-in release publishing continues
- for built-in crates.io publishing, `monochange step publish-readiness` now blocks packages whose current `Cargo.toml` cannot be published: `publish = false`, `publish = [...]` without `crates-io`, missing `description`, or missing both `license` and `license-file`
- workspace-inherited Cargo metadata such as `description = { workspace = true }` and `license = { workspace = true }` is accepted when `[workspace.package]` supplies the value
- already-published Cargo versions remain non-blocking and are skipped when current readiness and the saved readiness artifact agree
- monochange does **not** currently auto-configure `crates.io` trust; registry-side enrollment remains manual
- if you want the most literal crates.io/OIDC workflow today, `mode = "external"` plus `rust-lang/crates-io-auth-action@v1` is the clearest path

Recommended setup:

1. configure `trusted_publishing = true`
2. bootstrap missing release packages with `monochange step placeholder-publish --from HEAD --output .monochange/bootstrap-result.json` if needed, then rerun readiness
3. manually enroll the repository/workflow in `crates.io`
4. choose either:
   - `mode = "builtin"` and let monochange own the publish command, or
   - `mode = "external"` and use the official crates.io auth action directly

### GitHub + Deno / JSR trusted publishing

Config:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.deno.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
registry = "jsr"
```

Workflow sketch:

```yaml
name: publish-jsr

on:
  push:
    branches: [main]

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: detect monochange release commit
        shell: bash
        run: |
          set -euo pipefail
          if ! devenv shell -- monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "HEAD is not a monochange release commit; skipping publish"
            exit 0
          fi

      - name: publish JSR packages
        run: |
          devenv shell -- monochange step publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- monochange publish
```

Current behavior matches Cargo more than npm:

- monochange can validate the trust expectation and report the setup URL
- monochange does **not** auto-configure JSR trust on GitHub for you today
- manual registry enrollment is still required before the built-in publish can proceed

### GitHub + Dart / Flutter (`pub.dev`) trusted publishing

Config for monochange-managed release planning:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.dart.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
registry = "pub.dev"
```

monochange-oriented post-merge workflow sketch:

```yaml
name: publish-pub-dev

on:
  push:
    branches: [main]

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: detect monochange release commit
        shell: bash
        run: |
          set -euo pipefail
          if ! devenv shell -- monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "HEAD is not a monochange release commit; skipping publish"
            exit 0
          fi

      - name: publish pub.dev packages
        run: |
          devenv shell -- monochange step publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- monochange publish
```

More copy-pasteable registry-native example:

If you want the workflow shape recommended by the Dart team, prefer the reusable workflow from `dart-lang/setup-dart` and keep monochange focused on release planning. In that case, `mode = "external"` is usually the clearest fit.

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.dart.publish]
enabled = true
mode = "external"
trusted_publishing = true
registry = "pub.dev"
```

```yaml
name: publish-pub-dev

on:
  push:
    tags:
      - "my_package-v[0-9]+.[0-9]+.[0-9]+"

jobs:
  publish:
    permissions:
      id-token: write
    uses: dart-lang/setup-dart/.github/workflows/publish.yml@v1
    with:
      working-directory: packages/my_package
      # environment: pub.dev
```

If you need custom generation or build steps before publishing, switch to a custom workflow that runs `dart pub publish --force` or `flutter pub publish --force` after the OIDC-authenticated setup. For monorepos that mix package-specific tags, working directories, and external-mode jobs, see [Multi-package publishing patterns](./14-multi-package-publishing.md).

Current behavior:

- monochange can enforce the configured trust expectation
- monochange reports the manual setup URL when trust is not configured
- monochange does **not** auto-configure `pub.dev` trusted publishing today
- if you want the most copy-pasteable pub.dev flow today, `mode = "external"` plus the reusable `dart-lang/setup-dart` workflow is the clearest path

### GitHub post-merge package publish flow

If you want package publication to happen **after** the release PR merges, the simplest current pattern is:

1. merge the release PR so the monochange release commit lands on `main`
2. run `monochange step release-record --from HEAD --format json` in CI
3. if the command succeeds, run `monochange step publish-readiness --from HEAD --output .monochange/readiness.json`
4. run `monochange step publish-packages` only after readiness succeeds
5. if release-record detection or readiness fails, exit early before registry mutation

That pattern works well because `monochange step publish-readiness` and `monochange step publish-packages` consume the durable `ReleaseRecord` from `HEAD`; readiness gives you a reviewable preflight report, while `monochange step publish-packages` derives the publish work directly from release state before publishing.

## GitLab flows

### Current GitLab reality

GitLab is a supported source provider for hosted releases and release requests.

For package publishing, monochange can still run built-in package publication commands from GitLab CI, but the trust auto-derivation and npm `trust github` automation are GitHub-specific today.

That means the practical GitLab pattern is:

- keep `mode = "builtin"` when monochange's package publish command already matches what you need
- keep `trusted_publishing = false` unless the registry workflow is one you manage externally
- use CI secrets or external publishing logic when the registry requires a setup monochange does not automate on GitLab

### GitLab + npm

Config:

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
```

Workflow sketch:

```yaml
publish_npm:
  image: node:22
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - corepack enable
    - git fetch --force --tags origin
    - |
      set -euo pipefail
      if monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
        monochange step tag-release --from HEAD
        monochange step publish-readiness --from HEAD --output .monochange/readiness.json
        monochange publish
      else
        echo "not a release commit"
      fi
```

If your npm flow needs registry-token setup or a custom `.npmrc`, do that in CI before running `monochange step publish-readiness` and `monochange step publish-packages`.

### GitLab + Cargo

Config:

```toml
[ecosystems.cargo.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
```

Workflow sketch:

```yaml
publish_cargo:
  image: rust:1.90
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
      set -euo pipefail
      if monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
        monochange step tag-release --from HEAD
        monochange step publish-readiness --from HEAD --output .monochange/readiness.json
        monochange publish
      else
        echo "not a release commit"
      fi
```

If you need a crates.io token or a more customized release process, inject the credential in GitLab CI or switch the package to `mode = "external"`.

### GitLab + Deno / JSR

Config:

```toml
[ecosystems.deno.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
registry = "jsr"
```

Workflow sketch:

```yaml
publish_jsr:
  image: denoland/deno:latest
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
      set -euo pipefail
      if monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
        monochange step tag-release --from HEAD
        monochange step publish-readiness --from HEAD --output .monochange/readiness.json
        monochange publish
      else
        echo "not a release commit"
      fi
```

If your JSR auth bootstrap is more specialized than the built-in path expects, prefer `mode = "external"` and run the native publish command yourself.

### GitLab + Dart / Flutter

Config:

```toml
[ecosystems.dart.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
registry = "pub.dev"
```

Workflow sketch:

```yaml
publish_pub_dev:
  image: dart:stable
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
      set -euo pipefail
      if monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
        monochange step tag-release --from HEAD
        monochange step publish-readiness --from HEAD --output .monochange/readiness.json
        monochange publish
      else
        echo "not a release commit"
      fi
```

As with JSR, use `mode = "external"` when you need CI-specific auth or publish orchestration outside monochange's built-in assumptions.

## Long-running release PR branch flow

This is the flow you described:

1. every merge to `main` updates a dedicated release branch and PR
2. that branch contains the prepared release commit and release files
3. the release PR stays open and keeps tracking the latest releasable state
4. when the PR merges, publication happens from that merged release commit

### What monochange supports now

monochange now supports the core post-merge pieces of this shape directly:

- `monochange step open-release-request` can open or update a release request branch from current release state
- `monochange step commit-release` can create a durable monochange release commit with an embedded `ReleaseRecord`
- `monochange step release-record --from HEAD` can detect whether the latest commit is a monochange release commit
- `monochange step tag-release --from HEAD` can create and push the declared tag set from that merged release commit
- `monochange step publish-readiness` can write a readiness artifact from that same durable record on `HEAD`, and `monochange step publish-packages` can publish directly from the durable release record

### The important tag semantics

Tags are **not branch-scoped**.

A git tag points at a commit object, not at a branch name.

That means:

- if you create a tag on a release-PR commit, the tag exists immediately even before merge
- if that exact commit is later merged into `main`, the tag still points at the same commit and is now reachable from `main`
- if the release branch is later rebased, force-pushed, or regenerated, the old tag does **not** move automatically

That is why pre-merge tagging on a long-running release PR is usually the wrong move.

### Recommended workflow

For the long-running release PR model, the recommended shape is now:

1. on every push to `main`, run `monochange step open-release-request` to refresh the dedicated release PR branch
2. do **not** create tags on the release PR branch
3. merge the release PR when you are ready
4. on the post-merge workflow, run `monochange step release-record --from HEAD --format json`
5. if the latest commit is a release commit, run `monochange step tag-release --from HEAD`
6. after tags exist, run `monochange step publish-readiness --from HEAD --output <path>` and then `monochange step publish-packages` for package registries and let tag-triggered workflows create hosted releases or other downstream assets

That keeps tag creation on the default branch side of the merge, which is much safer than tagging the PR branch early.

### GitHub Actions reference sketch

```yaml
name: release

on:
  push:
    branches: [main]

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      id-token: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - name: fetch tags
        run: git fetch --force --tags origin

      - name: detect merged release commit
        id: release_record
        shell: bash
        run: |
          set -euo pipefail
          if monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
          else
            echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
          fi

      - name: refresh release PR
        if: steps.release_record.outputs.is_release_commit != 'true'
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: monochange step open-release-request

      - name: create release tags
        if: steps.release_record.outputs.is_release_commit == 'true'
        run: monochange step tag-release --from HEAD

      - name: publish packages
        if: steps.release_record.outputs.is_release_commit == 'true'
        run: |
          monochange step publish-readiness --from HEAD --output .monochange/readiness.json
          monochange publish
```

### GitLab CI reference sketch

```yaml
release_pr_or_publish:
  stage: release
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
      set -euo pipefail
      if monochange step release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
        monochange step tag-release --from HEAD
        monochange step publish-readiness --from HEAD --output .monochange/readiness.json
        monochange publish
      else
        monochange step open-release-request
      fi
```

## Choosing a CI pattern

Use this decision rule:

- **Need human review before release files land?** → use `monochange step open-release-request`
- **Need a durable local release commit?** → use `monochange step commit-release`
- **Need package registries after merge?** → detect `ReleaseRecord` on `HEAD`, run `monochange step tag-release --from HEAD`, then run `monochange step publish-readiness --from HEAD --output <path>` and `monochange step publish-packages`
- **Need hosted provider releases from prepared release state?** → use `monochange step publish-release`
- **Need to bootstrap release packages that do not exist yet?** → use `monochange step placeholder-publish --from HEAD --output <path>`; reserve names outside a release with lower-level `monochange step placeholder-publish`
- **Need GitHub npm trusted publishing with the least custom glue?** → use `trusted_publishing = true` with `monochange step publish-readiness` and `monochange step publish-packages`
- **Need GitLab CI with custom auth/bootstrap?** → keep `mode = "external"` as the escape hatch

## Related guides

- [GitHub automation](./08-github-automation.md)
- [Configuration reference](./04-configuration.md)
- [Release planning](./06-release-planning.md)
- [Repairable releases](./12-repairable-releases.md)
