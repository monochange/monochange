# Release pull request workflow

A release PR command is user-defined. `OpenReleaseRequest` needs `[source]`, so the example configures the provider first:

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"

[source.pull_requests]
base = "main"
title = "chore(release): prepare release"
labels = ["release", "automated"]

[cli.release-pr]
help_text = "Prepare a release pull request"

[[cli.release-pr.inputs]]
name = "format"
type = "choice"
choices = ["markdown", "json"]
default = "markdown"

[[cli.release-pr.steps]]
name = "plan release"
type = "PrepareRelease"
inputs = ["format"]

[[cli.release-pr.steps]]
name = "refresh lockfile"
type = "Command"
command = "pnpm install --lockfile-only"
# `when` skips the step when there is nothing to release, so an empty
# changeset set does not create an empty commit.
when = "{{ number_of_changesets > 0 }}"

[[cli.release-pr.steps]]
name = "create release commit"
type = "CommitRelease"
when = "{{ number_of_changesets > 0 }}"

[[cli.release-pr.steps]]
name = "open release request"
type = "OpenReleaseRequest"
inputs = ["format"]
when = "{{ number_of_changesets > 0 }}"
```

Preview with `monochange run release-pr --dry-run` before allowing the workflow to mutate branches. Dry-run mode performs every step's planning work without writing files, committing, pushing, or contacting the provider.
