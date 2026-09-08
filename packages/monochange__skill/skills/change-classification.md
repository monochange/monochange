# Choose changeset severity

Use this workflow before you create or edit a changeset for code, configuration, a CLI, a protocol, or another published behavior.

## Classify the change

1. Fetch the default branch and release tags when the checkout is shallow.
2. Run the canonical JSON command from the repository root:

   ```bash
   monochange change classify --format json --dependency-propagation public
   ```

3. If the CLI is unavailable and the monochange MCP server is configured, call `monochange_classify_changes` with `packages: []`, `detection_level: "signature"`, `include_unchanged: false`, and `dependency_propagation: "public"`.
4. Read every returned package. Finish only after each affected package has release intent or a documented review decision.

The command detects the remote default branch. Pass `--base <ref>` only when the detected branch is wrong. Pass `--release <ref>` to reproduce a known release baseline. Use repeated `--package <id-or-name>` flags only to narrow an investigation; classify the full change before final validation.

With the default `--head HEAD`, monochange materializes committed, staged, unstaged, deleted, and untracked files into one temporary Git candidate without changing the real index or worktree. Base the bump on `pullRequest`, which describes the net result after merge. Use `workingTree` only to explain the local part of that result; do not add its severity to the pull-request severity a second time.

## Read the decision

Use `decision.proposedChangesetBump` as the starting bump for the current pull request. Use `decision.releaseFloor` to understand all unreleased changes since the latest release. Do not copy `releaseFloor` into the current changeset when an earlier merged change caused it.

Trace `decision.findingIds` into `findings`. Confirm the source location, before and after signatures, and `comparisons` for each finding that determines a `major` or `minor` proposal.

The same item can have separate comparison-qualified findings when its signatures differ between the pull request and release intervals. Use only findings that include `pullRequest` when choosing the current changeset; use `release` findings to explain the accumulated release floor.

Interpret the remaining fields together:

- `compatibilityImpact: breaking` means a caller can require migration. Write a dedicated major changeset with the old and new usage.
- `compatibilityImpact: additive` means the public surface grew compatibly. Start with a minor changeset.
- `compatibilityImpact: compatible` means the modeled change is suitable for a patch.
- `compatibilityImpact: unknown` means the built-in analyzer could not classify the surface. Inspect the diff before choosing a bump.
- `reviewRequired: true` means the recommendation is advisory. Keep the proposed bump unless repository policy or stronger evidence justifies another choice.
- `completeness: complete` with `proposedChangesetBump: none` supports no release intent. A warning, unavailable comparison, or partial result requires review.

A `monochange/package-lifecycle` finding comes from manifest presence at both comparison endpoints. Treat a removed package as high-confidence breaking evidence and use a major changeset. This finding makes the bump decision complete because no higher bump exists. The finding remains available when the candidate also removes the package entry from `monochange.toml`. Use its preserved `releaseOwner.latestRelease` to inspect the release interval.

`action` describes the pending changeset work: `create`, `update`, `keep`, `review`, or `no_changeset`. For `review`, determine whether the changeset intentionally describes a cross-package consumer effect; remove it only after confirming that the release intent is stale. Read `existingChangesets` before adding a file so you do not duplicate release intent.

## Check ecosystem coverage

Package lifecycle findings are high-confidence and complete. Built-in Cargo, npm, Deno, and Dart source findings are medium-confidence and partial. The source analyzers model syntax and package metadata, but they do not prove every source-compatible behavior.

For a Rust breaking-change decision that needs stronger evidence, run cargo-semver-checks with the release tag from `releaseOwner.latestRelease`:

```bash
cargo semver-checks check-release \
	--manifest-path crates/example/Cargo.toml \
	--baseline-rev example/v1.2.3
```

Run the feature and target combinations that the crate supports. Reconcile cargo-semver-checks diagnostics with monochange findings. A clean run only covers the chosen configuration.

For TypeScript or JavaScript, inspect every published entrypoint and generated declaration. Check assignability when a parameter, return type, generic constraint, overload, export condition, or module format changed. For Rust, inspect `cfg`, features, traits, impls, and re-exports that are outside the analyzer's coverage note.

## Write and validate release intent

Write the changeset for the audience stream selected by its configured type. Put developer migration detail in the default stream and user-visible outcomes in a separate user stream when both audiences need the change.

Then run:

```bash
monochange step validate
monochange changeset validate --api --format markdown
monochange step prepare-release --dry-run --format json
```

The default API validation fails only on high-confidence evidence. To enforce every proposal after the repository has calibrated its analyzers, add `--strict`.

Complete the workflow when every affected package has the intended changeset action, both validations pass, and the dry-run release manifest shows the expected package, stream, output, and bump.

## Surface the decision on pull requests

Use the `change-classification` action when a repository should publish the same evidence for maintainers and agents:

```yaml
permissions:
  contents: read
  issues: write
  pull-requests: read

steps:
  - uses: actions/checkout@v6
    with:
      fetch-depth: 0
      ref: ${{ github.event.pull_request.head.sha }}
  - id: classify
    uses: monochange/actions/change-classification@v0
    with:
      dependency-propagation: public
```

Keep `fetch-depth: 0` so the classifier can resolve the merge base, default branch, and release tags. Check out the pull request head SHA so the candidate excludes GitHub's synthetic test-merge commit.

The action writes the full report to the job summary and exposes `json`, `markdown`, `recommendation`, `review-required`, and `summary` outputs. With `post-comment: true`, it also updates one marker comment rather than adding a new comment on every run. Treat `recommendation` as a routing hint only: read `json` and resolve every package whose `reviewRequired` is true before writing its changeset. Comment creation is best-effort so fork pull requests with read-only tokens still produce outputs and a job summary.

Until a monochange CLI release containing `change classify` is installed by the action, preinstall a compatible build and pass `setup-monochange: false`, or pass the executable command through `setup-monochange`.
