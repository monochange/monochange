# Advanced: Multi-package publishing patterns

This guide covers the practical publishing patterns that work well when one repository releases multiple packages across one or more ecosystems.

Use it when:

- one monochange workspace publishes more than one public package
- different registries need different publish triggers
- some packages stay on `mode = "builtin"` while others are clearer on `mode = "external"`
- trusted publishing must be enrolled per package instead of once per repository

## Start with the release boundary

For multi-package repositories, keep one idea fixed:

- monochange plans releases at the workspace level
- registries authorize publishing at the package level

That means the release plan can be shared, while publish automation often needs to stay package-specific.

A good default is:

1. let monochange prepare one release commit for the workspace
2. decide which packages use built-in publishing and which use external publishing
3. keep each registry's trusted-publishing enrollment aligned with the exact package workflow that will publish it

## Choose the simplest publish pattern that matches the registry

### Pattern 1: One post-merge publish job runs `monochange publish readiness` and `monochange publish packages`

Use this when most packages can stay on monochange's built-in publishing path.

```yaml
name: publish-packages

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

      - name: create release tags
        run: devenv shell -- monochange step tag-release --from HEAD

      - name: publish packages
        run: |
          devenv shell -- monochange publish readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- monochange publish packages --output .monochange/publish-result.json
```

This is the best fit when:

- multiple npm packages publish from the same workflow
- multiple packages share the same built-in post-merge flow
- you do not need package-specific tag triggers to satisfy the registry

A post-merge run on `main` cannot publish a trusted `pub.dev` package, because pub.dev only accepts a run on the package's own tag. monochange marks such a package `deferred_to_tag` instead of failing the run; publish it with the tag-driven shape in [Built-in pub.dev publishing from package tags](#built-in-pubdev-publishing-from-package-tags).

### Built-in publish order

When one `monochange publish packages` invocation contains multiple package publications, monochange publishes packages with no selected dependencies first, then publishes packages that depend on those packages, walking up the dependency tree until packages that depend on the most selected packages are published last.

The order is computed like this:

1. Build the selected publish requests from the prepared release or `HEAD` release state.
2. Materialize the workspace dependency graph.
3. Consider only dependencies where **both packages are part of the selected publish set**.
4. Ignore development dependency edges.
5. Topologically sort the publish requests so dependencies are emitted before dependents.

For example, with this internal package graph:

```text
core        # no dependencies
utils       # depends on core
api         # depends on utils
app         # depends on core, utils, api
```

monochange publishes in this order:

```text
core
utils
api
app
```

If multiple packages are independent at the same depth, their order is deterministic by package id, registry, and version.

A package with no selected dependencies is eligible first. A package is not published until all of its selected publish-relevant dependencies have been ordered before it. Dependencies outside the selected publish set do not block ordering. Development-only cycles are ignored. Runtime, build, peer, workspace, and unknown dependency cycles fail before publishing anything, with a cycle diagnostic.

### Built-in pub.dev publishing from package tags

pub.dev trusted publishing accepts an upload only from a GitHub Actions run whose ref is exactly `refs/tags/<package tag>`. Built-in publishing follows that rule, so one workflow can publish every registry:

1. Give each trusted pub.dev package a release tag: `tag = true` on the package, or on its group when it shares the group's tag. `monochange step validate` fails for a trusted pub.dev package whose releases create no tag.
2. On pub.dev, set the package's tag pattern to its release tag with `{{version}}` (for example `dart_client/v{{version}}`) and enable publishing from `push` and `workflow_dispatch` events. If the package requires a GitHub Actions environment, run the publish job in that environment.
3. Start one run of `monochange publish packages --output <path>` for every tag the release creates, from the tag pushes or by dispatching the workflow on each tag. monochange gives each package of the release to exactly one of those runs: a trusted pub.dev package to the run on its own tag, and every other package to the run on the primary tag (without one, to its own tag or the first tag that no trusted pub.dev package uses).
4. The run on the primary tag (without one, on that first tag) marks each trusted pub.dev package on another tag `deferred_to_tag`; its `deferred_to` object names the `reason`, the `tag`, the `ref`, and the `command` that dispatches the workflow on that tag. Every other run marks the packages it leaves to another tag's run `skipped_out_of_scope`, names that run, and does not look them up on their registries.

A run on a branch, or on a tag the release does not create, publishes as it would without pub.dev: only trusted pub.dev packages are deferred. A run that defers a package for a reason other than the tag (`wrong_event` or `unknown_ref`) ends with a warning, because no run of that workflow can publish the package until its trigger or `GITHUB_REF` is fixed.

| Status                 | Meaning                                                                                                                                                  | Fails the run? |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------- |
| `deferred_to_tag`      | pub.dev accepts this package only from a `push` or `workflow_dispatch` run on its own tag; that tag's run publishes it, or dispatch one with the command | no             |
| `skipped_out_of_scope` | another release tag's run publishes this package; the message names that run                                                                             | no             |

[CI, package publishing, and release PR flows](./13-ci-and-publishing.md#github--dart--flutter-pubdev-trusted-publishing) has a complete workflow that runs on every release tag and dispatches the deferred tags.

## Pattern 2: Package-specific external workflows publish from tags

Use this when the registry expects each package to have its own tag trigger, working directory, or workflow.

This is often the clearest fit for:

- `pub.dev`
- some `crates.io` setups
- mixed workspaces where one package needs registry-native steps that do not match `monochange publish packages`

Example tag naming scheme:

- `web-v{{version}}`
- `cli-v{{version}}`
- `dart_client-v{{version}}`

Example config:

```toml
[ecosystems.cargo.publish]
enabled = true
mode = "external"
trusted_publishing = true

[ecosystems.dart.publish]
enabled = true
mode = "external"
trusted_publishing = true
registry = "pub.dev"
```

Example workflow shape:

```yaml
name: publish-dart-client

on:
  push:
    tags:
      - "dart_client-v[0-9]+.[0-9]+.[0-9]+"

jobs:
  publish:
    permissions:
      id-token: write
    uses: dart-lang/setup-dart/.github/workflows/publish.yml@v1
    with:
      working-directory: packages/dart_client
      # environment: pub.dev
```

Choose this pattern when a tag for one package must never authorize publishing a different package.

## Pattern 3: One workflow, multiple package-specific jobs

Use this when you want one workflow file but separate jobs per package.

That gives you:

- one place to manage permissions and branch or tag triggers
- package-specific working directories
- package-specific environments
- package-specific failure visibility

Example shape:

```yaml
jobs:
  publish-crate-a:
    environment: crates-a
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
        id: auth
      - run: cargo publish --package crate_a
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}

  publish-crate-b:
    environment: crates-b
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
        id: auth
      - run: cargo publish --package crate_b
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
```

This pattern is especially useful when multiple packages live in the same ecosystem but should not share the same trusted-publishing enrollment.

## Registry-specific recommendations

| Registry  | Recommended multi-package pattern                                                                                      | Why                                                                   |
| --------- | ---------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| npm       | one post-merge `monochange publish readiness` + `monochange publish packages` job when possible                        | monochange can automate npm trusted-publishing setup on GitHub        |
| crates.io | one job per crate when using external OIDC auth                                                                        | trusted publishing is enrolled per crate and workflow context matters |
| jsr       | built-in `monochange publish readiness` + `monochange publish packages` is often fine, but keep setup package-specific | registry linking is still manual                                      |
| pub.dev   | package-specific tags: built-in publishing dispatched per deferred tag, or one external workflow per package           | automated publishing is tag-driven and package-specific               |

## Keep config, tags, and workflows aligned

For each published package, keep these values aligned:

- package id in `monochange.toml`
- registry package name
- trusted-publishing repository/workflow/environment values
- workflow trigger
- tag pattern, when the registry uses tags
- working directory, when the registry workflow publishes from a subdirectory

If those drift apart, trusted-publishing validation will be confusing even when release planning is correct.

## When to use package-level overrides

Use package-level publishing config when one package differs from the ecosystem default.

```toml
[ecosystems.dart.publish]
enabled = true
mode = "external"
trusted_publishing = true
registry = "pub.dev"

[package.dart_client.publish.trusted_publishing]
workflow = "publish-dart-client.yml"
environment = "pub.dev"

[package.example_app.publish]
enabled = false
```

This is the right move when:

- one package publishes from a different workflow file
- one package needs a protected environment but others do not
- one package is internal and should not publish publicly
- one ecosystem default is correct for most packages, but not all of them

## Practical rollout for an existing monorepo

1. decide which packages are public and which stay unpublished
2. choose `builtin` or `external` per ecosystem or package
3. register trusted publishing for each package at the registry
4. prefer package-specific tags where a registry is tag-authorized
5. run `monochange publish packages --dry-run` after registry enrollment changes
6. optionally run `monochange publish readiness --from HEAD --output <path>` as a preflight before real `monochange publish packages`
7. keep the workflow filename and environment stable once a registry record is enrolled

## Common mistakes

Avoid these failure modes:

- using one broad tag pattern that lets a tag for package A publish package B
- reusing one trusted-publishing record across packages that actually publish from different workflows
- changing a workflow filename after registry enrollment without updating the registry record
- keeping `mode = "builtin"` for packages that really need registry-native external publish steps
- forgetting that `pub.dev` automated publishing only accepts a run on the package's own tag; start a publish run for every tag the release creates, or dispatch the publish workflow for each `deferred_to_tag` package

## Related guides

- for registry-side trusted-publishing setup details, see [Trusted publishing and OIDC](./07-trusted-publishing.md)
- for end-to-end CI examples, see [CI, package publishing, and release PR flows](./13-ci-and-publishing.md)
- for publishing config fields and inheritance, see [Configuration reference](./04-configuration.md)
