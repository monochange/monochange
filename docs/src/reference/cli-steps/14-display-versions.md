# `DisplayVersions`

## What it does

`DisplayVersions` computes monochange's planned package and group versions and renders only that summary.

Use it when you want the release-version answer without the rest of the release preview.

## Why use it

Use `DisplayVersions` when you want a dedicated read-only command such as `monochange next`.

The built-in `monochange next` command (and its `monochange next-versions` alias) runs this step, prints the planned group and package versions, and writes nothing.

It is the best fit for:

- CI or local scripts that only need the planned version map
- release dashboards or follow-up tooling that want compact JSON
- human-readable summaries without release targets, changed files, or changelog previews
- answering "what version comes next" without preparing a release

Use `monochange versions list` instead when you want the versions recorded in the workspace today rather than the planned next versions.

## Inputs

- `format`: `text`, `markdown`, or `json`

## Empty changesets

An empty `.changeset` directory reports `no package or group versions were planned` and exits successfully instead of failing, so the step is safe to run between releases.

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

None. `DisplayVersions` is standalone.

## Side effects and outputs

`DisplayVersions` is read-only.

It:

- computes the same planned package and group versions used by monochange release workflows
- renders a compact summary in `text`, `markdown`, or `json`
- does not update manifests, changelogs, or consumed changesets
- does not write `release.json` or the prepared-release cache under `.monochange/local/`
- does not require a previous `PrepareRelease` step

## Example

```toml
[cli.versions]
help_text = "Display planned package and group versions"

[[cli.versions.inputs]]
name = "format"
type = "choice"
choices = ["text", "markdown", "json"]
default = "text"

[[cli.versions.steps]]
name = "display versions"
type = "DisplayVersions"
```

## Composition ideas

### Run the display step directly

```bash
monochange next
monochange next --format json
monochange step display-versions
monochange step display-versions --format markdown
monochange step display-versions --format json
```

### Keep release preparation and version display separate

Use `DisplayVersions` when you only need the version summary. Use [`PrepareRelease`](07-prepare-release.md) when you also need release file updates, release targets, manifest artifacts, or later release-oriented steps.

## Common mistakes

- expecting it to update release files
- treating it as a replacement for `PrepareRelease` in publish or release-request workflows
- bundling it into long multi-step commands when `monochange next` is clearer
- confusing it with `monochange versions list`, which reports current versions rather than planned next versions
