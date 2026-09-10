# Choose changeset severity

Use this workflow before you create or edit a changeset for code, configuration, a CLI, a protocol, or another published behavior.

## Classify the change

1. Fetch the default branch and release tags when the checkout is shallow.
2. Run the canonical JSON command from the repository root:

   ```bash
   monochange change classify --detection-level semantic --format json --dependency-propagation public
   ```

3. If the CLI is unavailable and the monochange MCP server is configured, call `monochange_classify_changes` with `packages: []`, `detection_level: "semantic"`, `include_unchanged: false`, and `dependency_propagation: "public"`.
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
- `compatibilityImpact: compatible` means the modeled public contract remains compatible. Use `proposedChangesetBump`: `none` means no declaration-driven bump, while a future analyzer may still recommend `patch` for compatible behavior.
- `compatibilityImpact: unknown` means the built-in analyzer could not classify the surface. Inspect the diff before choosing a bump.
- `reviewRequired: true` means the recommendation is advisory. Keep the proposed bump unless repository policy or stronger evidence justifies another choice.
- `completeness: complete` with `proposedChangesetBump: none` supports no release intent. A warning, unavailable comparison, or partial result requires review.

A `monochange/package-lifecycle` finding comes from manifest presence at both comparison endpoints. Treat a removed package as high-confidence breaking evidence and use a major changeset. This finding makes the bump decision complete because no higher bump exists. The finding remains available when the candidate also removes the package entry from `monochange.toml`. Use its preserved `releaseOwner.latestRelease` to inspect the release interval.

`action` describes the pending changeset work: `create`, `update`, `keep`, `review`, or `no_changeset`. For `review`, determine whether the changeset intentionally describes a cross-package consumer effect; remove it only after confirming that the release intent is stale. Read `existingChangesets` before adding a file so you do not duplicate release intent.

## Check ecosystem coverage

Package lifecycle findings are high-confidence and complete. Built-in Cargo, JavaScript, Deno, and Dart source findings are medium-confidence and partial. The source analyzers model syntax and package metadata, but they do not prove every source-compatible behavior.

For a TypeScript package, install `node`, the workspace's `typescript` compiler, and the package dependencies. Then use `detection_level: "semantic"`. monochange emits declarations independently for the before and after snapshots, resolves explicit `exports`, `types`, and `typings` entrypoints, and asks TypeScript to check consumer assignability.

Read these fields on every TypeScript finding:

- `analyzer.id: "npm/typescript"` confirms declaration comparison ran.
- `analyzer.engine` and `analyzer.version` identify the compiler that produced the evidence.
- `coverage.completeness: "complete"` means every explicit typed entrypoint in the declared scope was checked.
- `coverage.fallbackReason` explains missing config, dependencies, wildcard exports, or identity-sensitive declarations.

Treat a complete TypeScript `breaking` result as major and an `additive` result as minor. A complete `compatible` result can support `none` for declaration impact, but it does not prove runtime behavior. Review implementation semantics, side effects, and dynamic entrypoints separately. Keep `patch` and `reviewRequired: true` when the result is inconclusive.

Inherited config and dependency declarations may only exist in the current checkout rather than both Git snapshots. monochange uses them to keep the comparison useful but marks that evidence partial. Changed generic declarations and classes with private or protected members are also inconclusive when cross-snapshot identity prevents a sound comparison. Never upgrade partial evidence to complete from the absence of a diagnostic.

For high-assurance Rust classification, install cargo-semver-checks and enable the repository's explicit matrix:

```bash
cargo install cargo-semver-checks --locked
monochange change classify --detection-level semantic --format json --dependency-propagation public
```

```toml
[ecosystems.cargo.semver_checks]
enabled = true

[[ecosystems.cargo.semver_checks.matrix]]
name = "default"
feature_mode = "default"

[[ecosystems.cargo.semver_checks.matrix]]
name = "all-features"
feature_mode = "all"
```

Configure every supported feature/target combination, up to 16 uniquely named cells, and install each target before classification. Read `finding.coverage.checks` rather than inferring coverage from the overall bump. Each cell records its configuration, status, outcome, bump, and cargo-semver-checks lint diagnostics.

- Any checked cell that proves a break is sufficient for `major`, even if another cell fails.
- Only a fully checked, clean matrix can prove compatibility and replace syntax-derived removals or modifications.
- Syntax-derived additions remain `minor` because cargo-semver-checks does not enable every additive lint by default.
- A missing tool, target, failed build, timeout, skipped cell, or unrecognized tool response preserves the syntax fallback and requires review.

Do not translate “no diagnostic” into “no release” unless `coverage.completeness` is `complete`, every configured check has `status: "checked"`, and the package has no retained additive or runtime finding. The matrix proves only configured cargo-semver-checks rules and configurations; still inspect runtime behavior and supported configurations omitted from the matrix.

cargo-semver-checks executes Cargo builds, including build scripts and procedural macros, at both endpoints. Use it only for repository code you are prepared to execute. In CI, never combine semantic classification of untrusted changes with `pull_request_target`, registry credentials, or publishing secrets.

For JavaScript-only packages, inspect every published entrypoint manually. For TypeScript, inspect runtime behavior plus every fallback named in the report. For Rust, inspect `cfg`, features, traits, impls, and re-exports that are outside the analyzer's coverage note.

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
      detection-level: semantic
      dependency-propagation: public
```

Keep `fetch-depth: 0` so the classifier can resolve the merge base, default branch, and release tags. Check out the pull request head SHA so the candidate excludes GitHub's synthetic test-merge commit.

Install repository dependencies before the classification step when TypeScript packages publish declarations. The action writes the full report to the job summary and exposes `json`, `markdown`, `recommendation`, `review-required`, and `summary` outputs. With `post-comment: true`, it also updates one marker comment rather than adding a new comment on every run. The comment includes analyzer engine, version, completeness, coverage, and fallback reasons beneath each finding. Treat `recommendation` as a routing hint only: read `json` and resolve every package whose `reviewRequired` is true before writing its changeset. Comment creation is best-effort so fork pull requests with read-only tokens still produce outputs and a job summary.

The action accepts every report with classification `schemaVersion` 1 or newer, so new monochange CLI releases can extend the report without breaking the workflow.

To make the policy gate enforce classification in CI, give the `changeset-policy` action a `from` ref instead of an explicit path list:

```yaml
- uses: monochange/actions/changeset-policy@v0
  with:
    from: origin/main
```

With `from` set, a changeset whose bump is lower than the classified change type fails the check with the package name and both bumps; a higher bump only warns. Write changesets at `decision.proposedChangesetBump` or above so the policy passes.
