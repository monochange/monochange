# Cap the rendered release pull request body

## Status

- Scope: release pull request body rendering, provider request payloads, `[source.pull_requests]` configuration
- Outcome: the release request body can never exceed a provider body limit, and truncation is visible before the next create call fails
- Code changes: `monochange_core`, `monochange_config`, `monochange_hosting`, `monochange_github`, `monochange_gitlab`, `monochange_gitea`, `monochange_forgejo`, `monochange`

## Problem statement

`release_pull_request_body` renders the complete release notes for every release target. Nothing bounds the result.

GitHub rejects a pull request body above 65536 characters on create with a 422. The update path (`PATCH /pulls/{number}`) accepts far more, so a release pull request that stays open while changesets keep merging grows without limit and then blocks its own replacement:

```text
config error: GitHub API POST `/repos/{owner}/{repo}/pulls` failed: status 422;
details: {"code":"custom","field":"body","message":"body is too long (maximum is 65536 characters)","resource":"Issue"}
```

The failure surfaces as a raw provider error at step 5/5, with no hint that a configuration option exists. The notes that overflow the body are already published to `changelog.md`, the per-crate changelogs, the GitHub release body, and every configured changelog output, so bounding the request body removes release notes from no durable location.

## Expected behavior

- Every provider bounds the rendered body on both the create and the update path.
- An unconfigured repository is protected by default: the GitHub renderer caps the body at the provider's 65536 character limit, so a growing release pull request can no longer pass the cap through `PATCH`.
- `[source.pull_requests].body_style` selects how much of the notes the request body carries: `full` (default, current behavior) or `summary` (header, targets, and a pointer to the changelog paths).
- `[source.pull_requests].max_body_chars` overrides the provider default limit and applies in both modes.
- A truncated body keeps the header, the release-target list, and a visible pointer to the release changelog paths, and sets `body_truncation` on the `SourceChangeRequest` so the step output warns before the next create call.
- A rejected create call names the setting that fixes it instead of only echoing the provider payload.

## Decisions captured

- The bound lives in `monochange_hosting`, the shared renderer that GitHub, GitLab, Gitea, and Forgejo all reach. `monochange_github` keeps a private copy of `release_pull_request_body` and `release_pull_request_branch` that are byte-identical to the shared versions; both move to the shared path so the two copies cannot drift apart again.
- `body_style` defaults to `full`. Changing the default to `summary` would silently remove release notes from every existing repository's review surface; the automatic cap fixes the defect without that product change, and `body_style = "summary"` reaches the same end state deliberately.
- The default limit is per provider, because the limits are. GitHub is bounded at 65536 from the documented create-path rejection. GitLab, Gitea, and Forgejo have no verified body cap, so they bound only when `max_body_chars` is configured rather than inventing a limit that would truncate valid notes.
- Truncation drops whole release-note entries from the end and appends an explicit pointer to the changelog paths, so a truncated body never ends mid-sentence and never looks complete.
- The changelog pointer uses `ReleaseManifestChangelog.path`, the same durable files the notes were written to.
- `body_truncation` is advisory metadata on `SourceChangeRequest`, not a new step. The orchestrator already renders `context.release_request`, so the warning needs no new context field.

## Ordered checklist

- [x] Add `ProviderPullRequestBodyStyle` and the `body_style` / `max_body_chars` fields to `ProviderMergeRequestSettings` in `monochange_core`
- [x] Validate `max_body_chars` in `monochange_config`
- [x] Add bounded rendering (`render_release_pull_request_body`) and the limit hint to `monochange_hosting`
- [x] Add `body_truncation` to `SourceChangeRequest`
- [x] Route GitHub's private body and branch renderers through `monochange_hosting`
- [x] Pass style and limit from every provider request builder, with the GitHub default limit
- [x] Warn in the `OpenReleaseRequest` step output and JSON when the body was truncated
- [x] Surface `[source.pull_requests]` in rejected create errors
- [x] Regenerate schemas, artifact fixtures, docs, and the init template
- [x] Unit tests, integration fixture, snapshots, and changesets

## Outcome notes

- `release_pull_request_body(manifest)` keeps its signature and output byte-for-byte, so existing callers and snapshots are unaffected. Bounding happens in the new `render_release_pull_request_body` / `release_pull_request_body_for_source` entry points.
- A body that fits is rendered with no added pointer, so the GitHub default limit changes nothing until a release actually exceeds it.
- Truncation drops whole release-note sections from the end, then the changed-file list. A target heading whose notes were all dropped is removed rather than left dangling.
- The truncation notice inside the body is intentionally short, because its length is the floor the truncation loop cannot go below. The longer configuration advice lives in the step warning and the provider error.
- The GitHub default limit is 65536 characters from the documented create-path rejection. GitLab, Gitea, and Forgejo document no comparable limit, so they bound only when `max_body_chars` is set; inventing a limit there would truncate valid notes.
- Issue auto-close reads closing keywords from the contributor pull requests that introduced each changeset, not from the release request body, so bounding the body does not change release issue closure.

## Validation commands

```bash
devenv shell cargo test -p monochange_hosting --lib
devenv shell cargo test -p monochange_github --lib
devenv shell cargo test -p monochange_config --lib
devenv shell cargo test -p monochange_integration_tests
devenv shell lint:all
devenv shell snapshot:update
devenv shell monochange step validate
devenv shell coverage:patch
```

## Risks and boundaries

- Snapshot churn is expected wherever a release request payload is asserted.
- The `body_truncation` field changes `--format json` release request output; announce it in the developer changeset.
- `body_style = "summary"` must still list every outward target so a reviewer can see what the release contains without opening the notes.
- Issue auto-close reads closing keywords from the contributor pull requests that introduced each changeset, not from the release request body, so bounding the body does not change release issue closure.
