# Release-aware change classification

## Problem

monochange has semantic analyzers, API snapshots, CLI snapshots, changeset policy, and release history. These parts do not yet produce one trustworthy answer for an agent that needs to write a changeset. The current hidden `change classify` command compares one hard-coded ref range, derives confidence from the bump, and treats missing evidence as `none`.

The classifier must distinguish compatibility impact from release policy. It must also show whether evidence came from the pull request, the latest release, or the default branch.

## Scope

- Make `monochange change classify` a documented CLI command.
- Resolve the default branch and candidate revision instead of assuming `origin/main` and `HEAD`.
- Report pull request, release, release-to-default, and source-delta comparisons.
- Return stable findings with impact, bump, confidence, analyzer identity, locations, and comparison membership.
- Report analysis completeness separately from the proposed changeset bump.
- Compare the proposed bump with pending changesets.
- Include deletions and untracked files in local analysis.
- Reuse package path policy for `additional_paths` and `ignored_paths`.
- Update the CLI, MCP tool, docs, packaged skill, and generated agent guidance.
- Add a GitHub Action integration that posts or updates one pull request comment.
- Disable squash merging for the repositories changed by this work and use merge commits through the merge queue.

## Non-goals

- Claim complete language compatibility for Rust, TypeScript, JavaScript, or Dart.
- Fail CI on low-confidence or incomplete analysis by default.
- Create or edit changeset files as a side effect of classification.
- Publish packages or trigger release workflows.

## Delivery order

1. [x] Add regression tests for exact ref ranges, deleted files, and untracked files.
2. [x] Fix change-frame collection and path ownership.
3. [x] Add the versioned classification report and comparison resolver.
4. [x] Promote the command into the Clap tree and mirror the schema through MCP.
5. [x] Validate pending changesets against the enforceable minimum bump.
6. [x] Update reference docs, the packaged skill, and generated agent guidance.
7. [x] Run the full repository validation and patch-coverage gates.
8. [ ] Open the monochange PR and add it to the merge queue with merge commits.
9. [ ] Add the reusable pull request comment integration to `monochange/actions`.
10. [ ] Run the actions repository checks, open its PR, and add it to its merge queue.

## Acceptance checks

- `monochange change classify --format json` reports a schema version and all resolved comparisons.
- The report separates compatibility impact, proposed bump, enforceable minimum, confidence, and completeness.
- Every finding identifies its analyzer and the comparisons where it appears.
- Markdown output names the exact finding that caused each major or minor recommendation.
- `monochange changeset validate --api` checks pending changesets and returns a non-zero status for an enforceable mismatch.
- The local candidate includes committed, staged, unstaged, deleted, and untracked files.
- A local edit that reverts a committed branch change is classified from the net candidate, not the union of both diffs.
- `monochange --help` and command snapshots include `change classify`.
- The monochange skill tells an agent how to classify, interpret uncertainty, inspect both baselines, and write the changeset.
- The action posts one updatable pull request comment with package recommendations and finding evidence.
- `fix:all`, `build:all`, `lint:all`, `monochange step validate`, and `coverage:patch` pass.

## Risks

- A semantic analyzer can miss behavior that its snapshot does not model. Incomplete coverage must produce `reviewRequired: true`, never a confident `none`.
- A latest release tag can be absent or ambiguous. The report must retain the pull request comparison and mark the release comparison unavailable.
- Pull request permissions can prevent comments from forks. The action must still publish the Markdown report as output and a job summary.
