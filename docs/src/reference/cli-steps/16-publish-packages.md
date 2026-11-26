# `PublishPackages`

## What it does

`PublishPackages` publishes package versions to their target registries using monochange's built-in ecosystem workflows.

Run it from the command line with `monochange publish packages`. `monochange step publish-packages` runs the same step with the same inputs and remains supported for compatibility, but prefer `monochange publish packages` in CI, scripts, and docs.

The step derives publish work from durable monochange release state: a prepared release artifact when the command has one, or the release record discoverable from `HEAD` otherwise. It does not require a `readiness` artifact. Before publishing, it orders selected package publications by internal publish-relevant dependencies so dependencies are attempted before dependents. Runtime, build, peer, workspace, and unknown dependency kinds participate in ordering and cycle validation; development-only dependency cycles are ignored.

For each package with a planned release version, the step:

- resolves the registry from the package's publish configuration
- validates publish-relevant dependency cycles before registry mutation
- publishes dependencies before dependents within the selected publish set
- checks whether the version already exists (skipping if it does)
- defers a trusted pub.dev package to its own release tag when the run is on another ref (see [Packages that publish from their own tag](#packages-that-publish-from-their-own-tag))
- plans against registry rate limits before attempting any mutation
- runs the ecosystem-specific publish command (`cargo publish`, `npm publish`, `dart pub publish`, `flutter pub publish`, `deno publish`, and so on)
- produces a structured report of what was published, skipped, or planned

You can filter the publish set with the `package` input, or use an empty set to publish everything from the selected release state.

## Publication order

Package publication order is dependency-aware. monochange publishes packages with no selected dependencies first, then publishes packages that depend on those packages, walking up the dependency tree until packages that depend on the most selected packages are published last.

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

## Packages that publish from their own tag

pub.dev trusted publishing only accepts an upload from a GitHub Actions run that a `push` or `workflow_dispatch` event started on exactly `refs/tags/<package tag>`. Such a package is tag-bound: it uses the built-in publisher, trusted publishing, and no non-empty `PUB_TOKEN`. Before uploading a tag-bound package, the step compares the run's `GITHUB_REF`, `GITHUB_REF_TYPE`, and `GITHUB_EVENT_NAME` with that rule:

- **The run matches**: the package publishes.
- **The run does not match**: the package is not uploaded and gets the status `deferred_to_tag`. The run does not fail and the packages after it still publish. The report lists the package under **Deferred to release tags** with the tag and the command that publishes it; in `--format json` the package row carries `deferred_to` with `reason`, `tag`, `ref`, and `command`, and `summary.deferred` counts these packages. `reason` is `wrong_ref`, `wrong_ref_type`, `wrong_event`, or `unknown_ref`; for every reason but `wrong_ref`, the package's message says what to change in the workflow. No run of the workflow publishes a package deferred as `wrong_event` or `unknown_ref`, so the step's outcome is a warning that counts the packages left unpublished until the workflow's trigger or `GITHUB_REF` is fixed.
- **The run is on another of the release's tags**: the package is published by another tag's run. It gets `skipped_out_of_scope`, is not looked up on its registry, and is counted in `summary.out_of_scope`. Its message names the run that publishes it. A `--resume` plans these packages again instead of treating them as complete.

When a release has a tag-bound package, the step gives every package of the release to exactly one run on the release's tags, so a workflow that starts one run for each tag the release creates publishes each package once:

- A tag-bound package publishes in the run on its own tag.
- Every other package publishes in the run on the primary tag. Without a primary tag, it publishes on its own tag when no tag-bound package uses that tag, otherwise on the first tag of the release (in name order) that no tag-bound package uses, and otherwise on its own tag (or the release's first tag when it has none).
- The run on the primary tag (without one, on the first tag no tag-bound package uses) defers the tag-bound packages on other tags; every other tag's run marks them `skipped_out_of_scope`.

The step decides this over the whole release before it applies `--package`, `--group`, and `--ecosystem`, so a selection never moves a package into another run. Runs on branches and on tags the release does not create publish every package that is not tag-bound and defer the tag-bound ones. A release without tag-bound packages publishes every package on every ref, as before.

For example, on the primary tag:

```text
Would publish (1)
  web  1.1.0  npm

Deferred to release tags (1)
  app_dart  0.2.1  pub_dev  app_dart/v0.2.1

The registry only accepts these packages from a `push` or `workflow_dispatch` run on their release tag. Publish each one from its tag:
  gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1
```

```json
{
	"package": "app_dart",
	"registry": "pub_dev",
	"version": "0.2.1",
	"status": "deferred_to_tag",
	"deferred_to": {
		"reason": "wrong_ref",
		"tag": "app_dart/v0.2.1",
		"ref": "refs/tags/app_dart/v0.2.1",
		"command": "gh workflow run publish.yml --repo owner/repo --ref app_dart/v0.2.1"
	}
}
```

A real run refuses to start when a trusted pub.dev package has no release tag; set `tag = true` on the package or its group. Outside GitHub Actions the run's ref is unknown, so the rule does not apply. [How monochange publishes to pub.dev](../../guide/07-trusted-publishing.md#how-monochange-publishes-to-pubdev) covers the pub.dev settings, and [CI and publishing](../../guide/13-ci-and-publishing.md#github--dart--flutter-pubdev-trusted-publishing) a workflow that starts a run for every release tag and dispatches deferred tags.

## Why use it

Use `PublishPackages` when you want monochange to handle the full package-registry publication workflow rather than scripting individual publish commands.

That gives you:

- one publish step for all supported ecosystems
- automatic dependency ordering across internal package publications
- publish-relevant cycle detection before registry mutation
- automatic rate-limit planning and enforcement
- version-existence checks that prevent duplicate publish attempts
- dry-run previews that show the full publish plan without touching registries
- structured `publish.*` template context for later `Command` steps

Use `PlaceholderPublish` instead when you need to bootstrap a package that does not yet exist in its registry with a minimal `0.0.0` placeholder.

## Inputs

- `format`: `text`, `markdown`, or `json`
- `package`: optional repeated package ids used to filter the publish set
- `group`: optional repeated group ids; all packages in each group are added to the publish set
- `ecosystem`: optional repeated ecosystem names (`cargo`, `npm`, `deno`, `dart`, `python`, `go`; `flutter` is accepted as a legacy alias for `dart`); only packages targeting the selected ecosystems are published
- `resume`: optional path to a JSON result artifact from an earlier real `monochange publish packages` or configured publish workflow run; completed package versions are skipped and failed or pending work is retried
- `output`: optional path where monochange writes the package publish result JSON artifact for retry/resume workflows
- `show-all`: include every package's status, trusted-publishing metadata, command, and captured output in human-readable results

## Step-level `when` condition

All CLI steps support an optional `when = "..."` condition.

If the expression resolves to false at runtime, monochange skips the step and continues with the next step.

```toml
when = "{{ inputs.enabled }}"
```

## Step-level `always_run` flag

All CLI steps support an optional `always_run = true` flag.

When set, the step executes even if a previous step in the same command has failed. This is useful for cleanup, notification, or dry-run preview steps that must run regardless of earlier outcomes.

```toml
always_run = true
```

## Prerequisites

- a prepared release artifact or a release record discoverable from `HEAD` that contains the package publication targets
- no cycles among selected publish-relevant internal dependencies; development-only cycles are allowed
- for built-in Cargo publishes to crates.io, a publishable current `Cargo.toml`: no `publish = false`, any `publish = [...]` list includes `crates-io`, `description` is set, and either `license` or `license-file` is set; workspace-inherited values are accepted

## Side effects and outputs

- in dry-run mode, plans and previews publish operations without touching registries
- in normal mode, validates release-branch policy and publish-relevant dependency cycles, then publishes package versions to their configured registries
- ends human-readable output with the package versions published or planned and a summary line with explicit per-status counts
- keeps already-existing and external package rows, trusted-publishing metadata, commands, and captured output behind `--show-all`
- returns every package row when `--format json` is selected, regardless of `--show-all`
- when a registry refuses an upload because of its rate limit (HTTP 429), stops with `error[publish.rate_limited]`, names the registry's reason, the time it accepts uploads again, and its limits page, and records the same details under `rate_limit` (`reason`, `retry_at`, `docs_url`) on the failed package in `--format json`; the tool's own packaging log stays in the package's captured `stdout`/`stderr` instead of the terminal. Rerunning after the retry time resumes where the run stopped, because versions that already exist are skipped
- when `output` is set, writes the package publish result artifact even if a registry publish command fails, then exits non-zero for failed package outcomes
- contributes `publish.*` and `publish_rate_limits.*` template context to the command result

## Example

<!-- {=cliStepPublishPackagesExample} -->

```toml
[cli.publish]
help_text = "Publish package versions from monochange release state using built-in workflows"

[[cli.publish.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.publish.inputs]]
name = "package"
type = "string_list"

[[cli.publish.inputs]]
name = "group"
type = "string_list"
help_text = "Group ids whose member packages should be published"

[[cli.publish.inputs]]
name = "ecosystem"
type = "string_list"
help_text = "Ecosystems to publish (cargo, npm, deno, dart/flutter, python, go)"

[[cli.publish.inputs]]
name = "resume"
type = "path"
help_text = "JSON result artifact from an earlier package publish run; completed package versions are skipped"

[[cli.publish.inputs]]
name = "output"
type = "path"
help_text = "Write the package publish result JSON artifact for retry/resume"

[[cli.publish.inputs]]
name = "show-all"
type = "boolean"
help_text = "Include unchanged and skipped package details"

[[cli.publish.steps]]
name = "publish packages"
type = "PublishPackages"
inputs = ["format", "package", "group", "ecosystem", "resume", "output", "show-all"]
```

<!-- {/cliStepPublishPackagesExample} -->

## Composition ideas

### Preview readiness before publishing

Use `monochange publish readiness` when you want a reviewable preflight report, then publish directly from the same release state:

```bash
monochange publish readiness --from HEAD --output .monochange/readiness.json
monochange publish packages --output .monochange/publish-result.json
```

The readiness artifact is informational for `PublishPackages`; it is not required by `monochange publish packages`. If a real publish fails after writing `.monochange/publish-result.json`, fix the registry/auth issue and rerun with `monochange publish packages --resume .monochange/publish-result.json --output .monochange/publish-result.json`.

### Publish only a specific package

```toml
[cli.publish-core]
help_text = "Publish a specific package"

[[cli.publish-core.inputs]]
name = "package"
type = "string_list"
required = true

[[cli.publish-core.steps]]
name = "publish packages"
type = "PublishPackages"
```

### Publish with rate-limit planning

```toml
[cli.publish-planned]
help_text = "Plan and publish with rate-limit awareness"

[[cli.publish-planned.steps]]
name = "plan publish rate limits"
type = "PlanPublishRateLimits"

[[cli.publish-planned.steps]]
name = "publish packages"
type = "PublishPackages"
```

## Why choose it over a raw `Command` step?

Because `PublishPackages` understands:

- which packages were planned for release
- which ecosystem and registry each package targets
- which selected internal packages must publish before others
- whether publish-relevant dependency cycles would make a safe order impossible
- whether a version already exists (and should be skipped)
- ecosystem-specific publish commands, flags, and auth patterns
- rate-limit planning across registries
- dry-run behavior for safe CI previews
- trusted publishing setup and configuration

## Interaction with `publish.mode`

Packages with `publish.mode = "external"` are skipped by `PublishPackages`. If your CI or scripts handle publishing for a package, set `mode = "external"` to tell monochange not to publish that package during release publishing.

Placeholder publishing (`PlaceholderPublish`) is not affected by `publish.mode`. It processes all packages with `publish.enabled = true` regardless of mode. This is because placeholder publishing is a one-time bootstrap step, not a release publishing step.

## Common mistakes

- confusing `PublishPackages` with `PublishRelease`: the former publishes to package registries, the latter creates hosted provider releases (such as GitHub releases)
- assuming `monochange publish packages` consumes the JSON file from `monochange publish readiness`; use readiness for preflight review or `monochange step plan-publish-rate-limits --readiness`, not as a `PublishPackages` input
- omitting `output` in CI, which makes partial registry failures harder to resume safely
- expecting development-only dependency cycles to block publishing; only publish-relevant dependency kinds participate in cycle validation
- running `PublishPackages` without rate-limit planning: use `PlanPublishRateLimits` first when you are unsure about registry windows
