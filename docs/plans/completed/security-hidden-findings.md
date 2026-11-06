# Hidden security findings

All seven source repairs from scan `wfr_4f4edbf367c85f4a45b6fc00eb39bb1c561867ac3b290018f9742ae48f1fdaed` merged on 2026-10-28. The final source merge is `7307e37807c09b5bb89c3363795fff0fb4f7639e`.

| Finding                        | Shipped repair                                                      | Merged PR                                         |
| ------------------------------ | ------------------------------------------------------------------- | ------------------------------------------------- |
| `csf_115a582b7ddf586cd7800d46` | Authorize organizations through immutable provider IDs              | https://github.com/monochange/monochange/pull/846 |
| `csf_a538be758d878b8e797b724c` | Reject workflow-tree replacement through exact and ancestor paths   | https://github.com/monochange/monochange/pull/849 |
| `csf_13e192274124831fda28f307` | Bound JWKS refreshes independently of untrusted key IDs             | https://github.com/monochange/monochange/pull/848 |
| `csf_80874414b0497bc290c9843e` | Meter OAuth pages and consume state before provider exchange        | https://github.com/monochange/monochange/pull/847 |
| `csf_42b3da8761274e24c6c1e52d` | Compare complete release records before committing or deduplicating | https://github.com/monochange/monochange/pull/850 |
| `csf_2e164ea519dc74b8af363304` | Enforce release-branch policy before every branch mutation decision | https://github.com/monochange/monochange/pull/852 |
| `csf_34f0ef1ad827a8270282f55a` | Bind hosted PR updates to the intended release PR and repository    | https://github.com/monochange/monochange/pull/852 |

## Merge and coverage evidence

Every source PR had independent review, green current-head CI, resolved valid feedback, and 100% executable patch coverage before explicit submission to the protected main merge queue. All actual root and app merge-group checks subsequently completed successfully. Signed squash commits have the human author and no AI attribution; full attributed PR descriptions were restored and read back after merging.

| Repair                                 | Actual merge                               | Independently inspected executable patch coverage                          |
| -------------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------- |
| JWKS refreshes                         | `94f20ead1841219e45cff21801257168050995e3` | App 61/61                                                                  |
| Workflow integrity                     | `4abb6a129f81f7beaa22cda7609d93e64812910d` | App 16/16                                                                  |
| Organization identity                  | `eeaf90a563bdb3bb094c642bf36c1bb405ec4182` | App 144/144 new lines; 160/160 including workflow integrity                |
| OAuth limits                           | `3aee8b03925db096fed64d4a4a187c3e5c50ac73` | App 78/78 new lines; 238/238 including identity and workflow integrity     |
| Release-record integrity               | `49ea2400a63118554b2a99840fc38b7005a8096a` | CLI 40/40 across all five changed production files                         |
| Hosted branch policy and PR provenance | `7307e37807c09b5bb89c3363795fff0fb4f7639e` | App 133/133: GitHub release mutations 115/115 and scoped release API 18/18 |

These counts describe their respective comparison bases; they must not be added together. Every owned changed Rust production file has a unique, nonempty executable source map in the corresponding saved LCOV report. Root coverage alone does not verify app changes.

The [release-record merge-group root run](https://github.com/monochange/monochange/actions/runs/40720998536) completed 40 successful jobs and seven expected skips; [its app run](https://github.com/monochange/monochange/actions/runs/40720998480) passed all four jobs. The root suite passed 4,309 tests. The actual CLI LCOV SHA256 is `8e1bd67800210b1690bfc68b209c3331027e62358cdf080bd0e0dd2768360ef5`.

The [hosted authorization merge-group root run](https://github.com/monochange/monochange/actions/runs/40732884153) also completed 40 successful jobs and seven expected skips; [its app run](https://github.com/monochange/monochange/actions/runs/40732884027) passed all four jobs. The root suite passed 4,309 tests and the app workspace passed 719 tests with one existing ignored test. The actual app LCOV SHA256 is `901f01a60aa9b12cd793aec5f863873b912d7141e766ff79a06bc78d44c2868a`; the independent patch-audit SHA256 is `035f07b709ee9d43b294304d02b86c2a58ec15908ab7cb1759b7728ba62b9601`.

## Original-code controls

Controls executed the original vulnerable behavior and reached the intended assertions. Compilation failures and static review were not counted as reproductions.

| Repair                   | Actual original assertion failures                                                                                          |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------- |
| Organization identity    | 10: duplicate provider identity, mutable-login identity/linkage, and organization collision or victim-selection cases       |
| Workflow integrity       | 14: eight accepted protected-tree replacements and six accepted forbidden or noncanonical paths                             |
| JWKS refreshes           | 4: attacker-selected key IDs caused additional refresh requests                                                             |
| OAuth limits             | 14: nine route-alias limits, two missing anonymous limit headers, and three repeated provider exchanges                     |
| Release-record integrity | 20: sixteen content mutations, two serialization failures, tampered-record acceptance, and missing-record neighbor deletion |
| Hosted branch policy     | 8: forbidden branch creation or fast-forward decisions were accepted                                                        |
| Hosted PR provenance     | 1: an unrelated PR request returned 200 instead of 422                                                                      |

The [release-record controls](https://github.com/monochange/monochange/actions/runs/40609908058/job/122558648730) also reproduced both neighboring-record deletions when cleanup was deliberately moved before validation. Restoring the fixed validator passed 109 module tests at that historical checkpoint. Later permanent regressions cover cache origin, legacy recovery, and the shared commit boundary; the final full suite also retains the existing hosted routing and tag integration assertions.

The [hosted authorization controls](https://github.com/monochange/monochange/actions/runs/40668630171/job/122749470640) executed all nine branch-policy and provenance failures against the original source. Later main authorization interfaces made a whole-file historical source swap incompatible, so the temporary proof workflow was retired after preserving its tested sources and logs. All permanent regressions and normal CI remain.

## Integration and operational verification

- Prepared-release caches preserve the trusted producer identity for record comparison. Runtime routing, provider requests, and JSON output continue to describe the current consumer. Command tampering remains rejected; older caches require preparation through the original workflow, and changed producers require trusted restoration or explicit repair.
- Hosted provenance uses immutable GitHub repository IDs independently of local database row IDs. Additional router regressions cover provider/local ID mismatches and zero PR writes. Main's API-token scoping and token encryption remain intact.
- Named release previews were inspected against their manifests and configuration. CLI integrity notes use the main and CLI default outputs. Operator-only hosted authorization and reference notes appear in `website_developer/default`, at `app/developer-changelog.md`, and are absent from product Markdown and JSON.
- Existing website size and Lighthouse budgets passed without changes or exceptions. The final hosted merge-group log reports all eight routes within budget, with a minimum performance score of 90 and maximum TBT of 82 ms. The Criterion job emitted 38 distinct result rows. These results do not establish authenticated provider latency or a relative speedup.
- The OAuth auxiliary benchmark's setup timeout was resolved by one targeted retry on its actual merge commit; the retry executed all 38 Criterion cases. The whole-workspace formatter hook now omits filename batching. This removes redundant checks; the earlier stall's exact cause remains unproven.
- The maintainer authorized remaining validation in GitHub CI and waived local pre-push reruns. No new local heavy gates, package publication, tag or release mutation, release-workflow dispatch, or release-PR merge was performed.

## Evidence retention and limits

The [decision trail](security-hidden-findings-decisions.tsv) preserves checkpoint statements as recorded and appends their final outcomes. Logs, original source snapshots, LCOV reports, artifact digests, and independent audits are retained under `.monochange/local/security-verification/`. Preserve useful evidence before removing an owned worktree. Unrelated worktrees, running jobs, and pre-existing dirty files remain outside cleanup scope.

Two final merge-group performance ZIP downloads encountered a cache connection timeout. Their actual completed job logs were read, and bounded measurement excerpts and remote artifact IDs/digests were preserved instead; those ZIPs are not described as locally verified. The app coverage ZIP and executable maps were downloaded and independently verified.

The imported findings remain unlisted in Security Cloud. These are verified source-repair outcomes, not claims that canonical Cloud findings were closed. Local evidence retention and final owned-worktree cleanup are tracked in the private coordination ledger after this archive change lands.
