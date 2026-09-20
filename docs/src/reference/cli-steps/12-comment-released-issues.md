# `CommentReleasedIssues`

## What it does

`CommentReleasedIssues` uses prepared release context to comment on issues linked from the release's changeset and review metadata.

It is a post-publication communication step, not a planning step.

## Why use it

Use `CommentReleasedIssues` when you want monochange to close the loop after publication by posting structured release follow-up comments.

This is especially valuable when:

- issues are part of the public release workflow
- you want issue comments to stay tied to the exact prepared release data
- you want a dry-run preview before touching hosted issue state

## Inputs

- `format`: `text` or `json`
- `from-ref`: git ref that contains the release record to publish
- `auto-close-issues`: close issues that the release review requests claim via closing keywords after adding the release comment

## Issue closure

With `auto-close-issues` enabled, only issues referenced through closing keywords (`Closes #7`, `Fixes #8`, …) in the release review request bodies are closed, and closure is attempted even when the issue is already closed, because hosted forges only auto-close the first issue of a comma-separated `Closes #7, #8` list. Issues that are merely mentioned without a closing keyword are never closed; add a closing keyword to the release pull request body when a mention should close with the release.

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

- a previous `PrepareRelease` step in the same command
- `[source].provider = "github"`

## Side effects and outputs

- builds issue comment plans from prepared release context
- in dry-run mode, previews which issues would be touched
- in normal mode, creates or skips comments based on provider state

## Example

<!-- {=cliStepCommentReleasedIssuesExample} -->

```toml
[cli.publish-and-comment]
help_text = "Publish a release and comment on linked issues"

[[cli.publish-and-comment.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "json-min"]
default = "text"

[[cli.publish-and-comment.steps]]
type = "PrepareRelease"
inputs = ["format"]

[[cli.publish-and-comment.steps]]
type = "PublishRelease"
inputs = ["format"]

[[cli.publish-and-comment.steps]]
type = "CommentReleasedIssues"
```

<!-- {/cliStepCommentReleasedIssuesExample} -->

## Composition ideas

### Publish first, then comment

The most common and most sensible sequence is:

1. `PrepareRelease`
2. `PublishRelease`
3. `CommentReleasedIssues`

That ordering reflects the real-world intent: only comment after the release event exists.

### Comment and then run a reporting step

```toml
[cli.publish-comment-report]
help_text = "Publish a release, comment on issues, and print a short report"

[[cli.publish-comment-report.steps]]
type = "PrepareRelease"

[[cli.publish-comment-report.steps]]
type = "PublishRelease"

[[cli.publish-comment-report.steps]]
type = "CommentReleasedIssues"

[[cli.publish-comment-report.steps]]
type = "Command"
command = "echo issue comments processed for {{ release.version }}"
shell = true
```

## Why choose it over a custom GitHub API script?

Because the built-in step already consumes monochange's linked issue and review metadata model. A shell script would need to rediscover which issues matter for the release.

## Common mistake

Using `CommentReleasedIssues` without a GitHub source configuration. This step is intentionally provider-specific.
