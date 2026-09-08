# Change classification

`monochange change classify` produces a release-aware severity report for packages affected by a pull request or local worktree.

## Comparisons

The classifier resolves these comparisons:

| Kind               | Base                                                             | Head                                                | Purpose                                                     |
| ------------------ | ---------------------------------------------------------------- | --------------------------------------------------- | ----------------------------------------------------------- |
| `pullRequest`      | The remote default branch, or `--base`                           | The synthetic merge result of the base and `--head` | Changes introduced after the pull request merges            |
| `sourceDelta`      | The merge base of the default branch and the source candidate    | The source candidate                                | Changes authored on the pull request branch                 |
| `workingTree`      | `HEAD`                                                           | The staged, unstaged, deleted, and untracked files  | A diagnostic view of local changes                          |
| `release`          | The package release owner's latest reachable tag, or `--release` | The synthetic merge result                          | Accumulated change since the latest release                 |
| `releaseToDefault` | The same release tag                                             | The default branch                                  | Change that has already accumulated before the pull request |

`pullRequest` and `release` are exact two-endpoint comparisons. `sourceDelta` uses the merge base only to identify work authored on the branch. When `--head` is `HEAD`, the source candidate materializes committed and local changes into one temporary Git commit. The bump comes from the net candidate comparison, while `workingTree` remains a diagnostic view. A local edit that reverses a committed breaking change therefore removes that break from the proposal instead of adding a second, contradictory signal.

The candidate is a tree object created by `git merge-tree --write-tree`. monochange uses a temporary index for local changes and does not change the real index or worktree. If Git cannot create the merge tree, the report marks the comparison as `conflicted`, falls back to the source candidate, and requires human review.

## Package decision

Each package has a `decision` object with these fields:

| Field                   | Meaning                                                                           |
| ----------------------- | --------------------------------------------------------------------------------- |
| `compatibilityImpact`   | `breaking`, `additive`, `compatible`, or `unknown` for the current pull request   |
| `proposedChangesetBump` | The highest current finding: `major`, `minor`, `patch`, or `none`                 |
| `enforceableMinimum`    | The highest bump supported by high-confidence evidence                            |
| `releaseFloor`          | The highest bump found between the latest release and the candidate               |
| `confidence`            | Confidence of the finding that determines `proposedChangesetBump`                 |
| `completeness`          | Whether the analyzer claims `complete`, `partial`, or `unsupported` coverage      |
| `reviewRequired`        | Whether the proposal needs human or agent review before it becomes release intent |
| `findingIds`            | Stable identifiers for the findings that determine the proposal                   |

`proposedChangesetBump` describes the current pull request. `releaseFloor` describes the complete unreleased interval. A pull request can propose `patch` while the release floor is `major` because an earlier merged pull request introduced the breaking change.

`none` is conclusive only when `completeness` is `complete` and `reviewRequired` is `false`. A changed package without modeled semantic evidence receives a low-confidence `patch` proposal instead of a false `none` result.

The package-level `action` is `create`, `update`, `keep`, `review`, or `no_changeset`. The report includes packages targeted only by a pending changeset and marks them `review`; a changeset can intentionally describe a consumer-facing effect implemented in another package, so monochange does not assume that unmatched intent is stale. Public dependency propagation retains the dependent package's release owner, comparisons, and existing changesets.

## Findings

Each finding records its `ruleId`, API surface, change kind, compatibility impact, bump, confidence, analyzer id and version, coverage note, source location, before and after signatures, and comparison membership.

Identical evidence found in several comparisons shares one finding and lists every comparison. If the same item has different before or after signatures across the pull-request and release intervals, monochange emits distinct comparison-qualified finding ids so that an agent never applies one interval's signature evidence to another interval.

The built-in Cargo, npm, Deno, and Dart analyzers inspect syntax and package metadata. Their findings are `partial` and medium-confidence because they do not prove every language compatibility rule. For example, the Rust analyzer does not model every `cfg` and feature combination, trait compatibility rule, or downstream build witness. The TypeScript and JavaScript analyzer does not yet run the TypeScript assignability checker for every exported entrypoint.

When the repository defines `[package.*]` entries, classification is limited to those configured packages. Package `additional_paths` and `ignored_paths`, plus `[changesets.affected].ignored_paths`, use the same path policy as changeset coverage. This keeps fixtures, tests, generated output, and other explicitly ignored paths from producing release recommendations.

Package discovery reads the candidate checkout. If a pull request removes an entire package, including its manifest, monochange cannot reconstruct that package's identity from the current checkout. Treat a deleted manifest as a manual-review signal and compare the package from the base or release ref before choosing its final changeset.

For a higher-assurance Rust check, run cargo-semver-checks against the resolved release tag and the current manifest:

```bash
cargo semver-checks check-release \
	--manifest-path crates/example/Cargo.toml \
	--baseline-rev example/v1.2.3
```

cargo-semver-checks reports compatibility violations across the selected feature and target configuration. Run extra feature or target combinations when those combinations are part of the supported API. monochange does not currently import this external result, so reconcile its diagnostics with the matching monochange findings before choosing the changeset bump.

## Output and validation

Markdown output is intended for terminal output, pull request comments, and job summaries:

```bash
monochange change classify --format markdown --dependency-propagation public
```

JSON is the stable agent and automation interface. The top-level `schemaVersion` changes when the JSON contract changes:

```bash
monochange change classify --format json --dependency-propagation public
```

`monochange changeset validate --api` fails only when a pending changeset is lower than `enforceableMinimum`. `--strict` compares pending changesets with `proposedChangesetBump`, including partial and medium-confidence evidence.

```bash
monochange changeset validate --api --format markdown
monochange changeset validate --api --strict --format markdown
```

The `monochange_classify_changes` MCP tool returns the same report under its `report` field. It accepts `base`, `head`, `release`, `packages`, `detection_level`, `include_unchanged`, and `dependency_propagation` inputs. Agents should use `dependency_propagation: "public"` for the same package coverage as the canonical CLI workflow.

## CI requirements

Release comparison needs the relevant tags and history. A shallow checkout can make the latest release unavailable or produce an incomplete merge base. CI jobs that publish classification reports fetch the default branch and tags before running the command.

Pull request comments from forked repositories can lack write permission. A classification job can always write the Markdown report to the job summary and expose it as an output even when the provider refuses the comment.
