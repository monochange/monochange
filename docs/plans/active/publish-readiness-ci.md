# Publish readiness CI

## Why

The release workflow should not be the first place publish-readiness errors are discovered. The release job added for PR #524 protects the final publication path, but it only runs for release tags or manual release dispatches. A normal pull request can therefore stay green while the publish-readiness command would fail.

## Goals

- Run publish readiness as a normal CI check on every pull request update, merge queue run, and push covered by the CI workflow.
- Keep the release workflow publish-readiness gate before draft publication and before dispatching package publication.
- Make the `publish:check` devenv script fail on real publish-readiness blockers instead of swallowing failures.
- Keep the check non-publishing: it may perform dry-run/readiness work, but must not publish packages.

## Decisions

- Reuse `publish:check` as the CI entry point so local and CI validation execute the same command.
- Store the JSON report at `target/publish-readiness.json` by default, with `MONOCHANGE_PUBLISH_READINESS_OUTPUT` as an override.
- Add a separate `publish-readiness` CI job rather than hiding the readiness gate inside an existing build or packaging job.

## Progress

- [x] Added a dedicated `publish-readiness` job to `.github/workflows/ci.yml`.
- [x] Changed `publish:check` to run `mc step:publish-readiness --from HEAD --output "$output" --format json` without `|| true`.
- [x] Validated formatting and whitespace locally with `dprint check` and `git diff --check`.
- [x] Confirmed `publish:check` now fails locally on the known cyclic publish dependency blocker instead of swallowing the error.
- [ ] Push the PR #524 update and monitor the new check.

## Follow-up

The current mainline publish-readiness graph still depends on PR #525 to remove the `monochange_core` → `monochange_test_helpers` dev-dependency cycle. Until that lands or the branch includes that fix, the new CI job should fail for the same blocker that already breaks publication.
