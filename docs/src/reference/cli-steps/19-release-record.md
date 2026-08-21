# `ReleaseRecord`

## What it does

`ReleaseRecord` inspects the monochange release record embedded in a release commit.

It resolves the supplied ref to a commit, walks first-parent ancestry until it finds a release-record block, and renders the recorded targets, package versions, changed files, changelogs, and release metadata.

## Why use it

Use `ReleaseRecord` when you need to answer "what did monochange release from this commit or tag?" without re-planning from current workspace files.

It is especially useful for:

- debugging publication or tag automation after a release commit exists
- checking release metadata before tag repair or provider publication
- exporting the embedded record as JSON for external tooling

## Inputs

- `from`: required tag or commit-ish used to locate the release record
- `format`: `text` or `json` output, defaulting to `text`

## Prerequisites

The selected ref, or one of its first-parent ancestors, must contain a valid monochange release record embedded by `CommitRelease`.

## Side effects and outputs

`ReleaseRecord` is read-only. It fails loudly when a malformed release record block is found, because later tag and publish workflows depend on that record being trustworthy.

## Example

```bash
monochange step release-record --from v1.2.3
monochange step release-record --from HEAD --format json
```

Use it before tag repair or package-publish planning when you need to confirm the exact release state that downstream commands will consume.
