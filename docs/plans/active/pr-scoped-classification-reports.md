# Keep change classification reports specific to the pull request

Status: in progress.

Reported on [pina-rs/pina#573](https://github.com/pina-rs/pina/pull/573#issuecomment-5911528007).

## Problem

The classification comment on a pull request lists findings for files the pull request never touched, and it does not say which commit it describes.

1. **The comment renders default-branch evidence as pull request evidence.** `monochange change classify` deliberately keeps `release` and `releaseToDefault` findings in each package's `findings` array so the release floor is auditable, and `decision.pull_request_changes` already keeps them out of the proposed bump. The Markdown renderers (the CLI's own, and the one in `monochange/actions`) still count and list every finding as if the pull request produced it. On pina-rs/pina#573, nine of eleven `pina` findings were `seen in release, release_to_default` and named `src/migration.rs`, `src/entry.rs`, `src/error.rs`, and `src/traits.rs`, none of which are in the pull request.
2. **A stacked pull request is compared with the default branch.** The `change-classification` action only passes `--base` when the caller sets it, so a pull request whose base is another feature branch is classified against `origin/main` and inherits every change from the branch below it.
3. **Nothing in the report identifies the commit it classified.** The report names the merge tree (`merge-tree:<tree>`) and `HEAD`, neither of which a reader can match to the pull request, so a stale comment is indistinguishable from a current one.

## Scope

- Record the resolved `base_commit` and `head_commit` in the classification report (schema `0.4`, additive).
- Render pull request findings and inherited base-branch findings separately in the CLI Markdown and text reports, and count only pull request findings as the package's findings.
- Make the GitHub action default `--base` to the pull request base branch and `--head` to the pull request head commit from the event payload, and render the same split with the classified commit in the comment.
- Re-run classification when a pull request is retargeted, so a stacked pull request refreshes after the branch below it merges.
- Update reference docs, the packaged skill, fixtures, snapshots, and changesets.

## Non-goals

- Changing the JSON `findings` contract. Each finding's `comparisons` set already records the interval it came from, and the release floor needs the inherited evidence.
- Changing how the proposed bump or the release floor is computed.

## Design

### Report endpoints

`build_change_classification_report` resolves `--base` and `--head` to commit ids with `git rev-parse --verify` and stores them as `base_commit` and `head_commit`. A report built from a single analysis frame (`api diff`) or a skipped run leaves both absent.

### Pull request findings

A finding belongs to the pull request when its `comparisons` include `pullRequest`, `sourceDelta`, or `workingTree`. A finding seen only in `release` and `releaseToDefault` describes work the base branch already carries. Renderers list the first group as the package's findings and the second group under "Unreleased changes already on `<base>` (not part of this pull request)".

### Action defaults

When the `base` input is empty and the event payload has `pull_request.base.ref`, the action passes `--base origin/<ref>`. When the `head` input is empty and the payload has `pull_request.head.sha`, it passes `--head <sha>`, so the classification names the pull request head even when the checkout is GitHub's merge ref. The comment opens with the classified head commit and base commit.

Recommended workflows add the `edited` pull request event with a guard on `github.event.changes.base`, so a retargeted stacked pull request is reclassified against its new base.

## Affected files

- `crates/monochange_classification/src/classification.rs`
- `crates/monochange_classification/src/__tests__/classification_tests.rs`
- `crates/monochange_classification/SCHEMA_VERSION`, schema assets under `crates/monochange_classification/schemas/` and `docs/src/schemas/`
- `crates/monochange_integration_tests/tests/api_classification.rs` and snapshots
- `crates/monochange/src/cli.rs` help text for `--base`
- `docs/src/reference/change-classification.md`, `packages/monochange__skill/skills/change-classification.md`
- `.github/workflows/change-classification.yml`
- `monochange/actions`: `src/actions/change-classification/index.ts`, `action.yml`, `change-classification/README.md`

## Validation

- `devenv shell -- cargo test -p monochange_classification --all-features`
- `devenv shell -- cargo test -p monochange_integration_tests --test api_classification`
- `devenv shell schema:update`, `devenv shell snapshot:update`
- `devenv shell fix:all`, `devenv shell lint:all`, `devenv shell coverage:patch`
- `pnpm all` in `monochange/actions`

## Follow-ups

- Downstream repositories re-pin `monochange/actions/change-classification` once the action release is tagged, and add the `edited` trigger to their classification workflow.
