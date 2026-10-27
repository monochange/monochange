# Hidden security findings

Track the seven findings from scan `wfr_4f4edbf367c85f4a45b6fc00eb39bb1c561867ac3b290018f9742ae48f1fdaed` against current main. Reports are evidence; verify each claim against code before changing it.

| Finding                        | Required outcome                                                  | Status                                                                                 |
| ------------------------------ | ----------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `csf_42b3da8761274e24c6c1e52d` | Compare complete release-record content before committing         | Implemented; original-code and cleanup-order controls awaiting CI                      |
| `csf_2e164ea519dc74b8af363304` | Enforce release-branch policy before every branch update decision | Implemented; tests pending                                                             |
| `csf_a538be758d878b8e797b724c` | Reject workflow-tree replacement through exact and ancestor paths | Fourteen original failures reproduced; 182 API tests green; remaining CI pending       |
| `csf_34f0ef1ad827a8270282f55a` | Bind hosted PR updates to the intended release PR                 | Implemented; tests pending                                                             |
| `csf_115a582b7ddf586cd7800d46` | Authorize organizations through immutable IDs                     | Ten original failures reproduced; 601 app-workspace tests green; coverage pending      |
| `csf_13e192274124831fda28f307` | Bound JWKS refreshes independently of untrusted key IDs           | Four original failures; 604 app-workspace tests green; executable patch coverage 61/61 |
| `csf_80874414b0497bc290c9843e` | Meter OAuth pages and consume state before provider exchange      | Implemented; original-code control and fixed suite awaiting CI                         |

Use focused sibling worktrees and PRs for release-record integrity, hosted release authorization, tenant identity, JWKS refresh policy, and OAuth resource limits. Preserve unrelated checkouts. Remaining builds, tests, lint, and coverage run in GitHub CI with bounded cargo jobs; local pre-push reruns are waived for this repair batch.

## Published priority fixes

The first batch prioritizes organization identity, workflow integrity, JWKS refreshes, and OAuth limits. The remaining checks run in GitHub CI, including deliberate original-code controls and app-workspace coverage. Root coverage alone cannot verify app changes.

- Organization identity: https://github.com/monochange/monochange/pull/846
- Workflow integrity: https://github.com/monochange/monochange/pull/849
- JWKS refreshes: https://github.com/monochange/monochange/pull/848
- OAuth limits: https://github.com/monochange/monochange/pull/847

These PRs are awaiting current-head CI and merge. Release-record integrity, release-branch policy, and PR provenance remain required. Workflow integrity was separated from the latter two changes so the first batch can land independently.

## Execution checklist

- [x] Read the figure-it-out principles and frame the seven-code queue.
- [x] Check the initial baseline and existing PRs: main was `05d0753505a07ef9bf9bb0fef6e4713da1773ddd`, with only the action-pin PR open at the initial check.
- [x] Define five independent units with exploit regressions.
- [x] Fan out tenant identity, JWKS, and OAuth work into owned sibling worktrees. Keep release-record and hosted-release changes with the coordinator.
- [ ] Verify each unit against its exploit and failure paths, then review its diff independently.
- [ ] Run the required checks and patch coverage for each current PR head before merging.
- [ ] Aggregate the seven code outcomes, merge green reviewed PRs, and remove only owned merged worktrees.
- [ ] Audit the decision trail and report any verification gaps.

Done means each finding has either a merged fix with a regression that fails on the original code, or concrete proof that current main already closes the path. Static review alone cannot establish a fixed outcome. This run uses security review and actual mutation-boundary tests because the affected paths authorize releases and tenant access.

For each fix: reproduce the failure, implement the smallest sound change, run focused tests and failure cases, run repository formatting and required validation, reach 100% executable patch coverage, review the full diff, sign and verify commits, open a PR, address review and CI, then explicitly merge through the repository queue. Never publish, tag, trigger release workflows, or merge release PRs.
