# `TagRelease`

## What it does

`TagRelease` creates the release tags declared by a monochange release record.

It reads the record embedded in the selected release commit, creates the full tag set from that record, and pushes tags to `origin` by default. Re-running it on the same commit is treated as already up to date.

## Why use it

Use `TagRelease` when tag creation is separated from release-commit creation or when CI should make the tag side effect explicit.

It is especially useful for:

- publishing all package and group tags from one authoritative release record
- previewing tag names before creating or pushing them
- enforcing `[source.releases]` branch policy before tags are created
- keeping tag creation idempotent for retryable CI jobs

## Inputs

- `from` — required release commit ref
- `push` — boolean, defaulting to `true`; set `--push=false` to create tags locally without pushing
- `dry_run` — preview without creating or pushing tags
- `format` — `text` or `json`, defaulting to `text`

## Prerequisites

The resolved `from` ref must be the monochange release commit itself, not just a descendant that can find a release record by ancestry.

If `[source.releases]` sets `enforce_for_tags = true`, the release commit must satisfy the configured release-branch policy before tags are created.

## Side effects and outputs

In normal mode, `TagRelease` creates local git tags and pushes them to `origin` unless `push = false`. In dry-run mode, it only previews the tags that would be created.

Do not use this step to repair an already-published tag set. Use `RetargetRelease` for explicit repair workflows.

## Example

```bash
mc step:tag-release --from HEAD
mc step:tag-release --from HEAD --dry-run
mc step:tag-release --from HEAD --push=false
mc step:tag-release --from HEAD --dry-run --format json
```

In a release workflow, run it after `CommitRelease` has produced the release commit and after any branch-policy validation you want to perform explicitly.
