# Findings: monochange CLI and skill evaluations

This record distinguishes authored coverage, executed evidence, and conclusions that remain pending. It does not establish that monochange has no hidden bugs.

## Current question and method

Can an agent use the shipped guidance to initialize and update realistic monorepos, choose configuration and release intent correctly, recover from diagnostics, and complete requested local preparation while respecting publication boundaries? Does a shorter skill entrypoint produce better outcomes than the shipping guidance on the same tasks?

The committed matrix contains **60 scenarios: 38 agent tasks and 22 agent-free contracts, with 315 checks across 23 fixture families**. Adoption, ecosystem, planning, and guardrail tasks cover Cargo, npm, Deno JSON/JSONC, Dart/Flutter, PEP 621/Poetry Python, and Go. COVERAGE.md lists every scenario and remaining gaps. Contracts pin CLI behavior without model calls; they do not score skill quality.

Each run gets a copied fixture and its own Git root. The agent receives a realistic request, the selected skill channel, and repository policy. Checks grade observable artifacts in a separate copy so mutating graders do not consume saved agent results. Transcript checks use assistant-authored text and tool inputs, excluding skill text and tool responses. Runtime errors, authentication failures, timeouts, nonzero exits, and missing successful terminal results fail execution independently of artifact checks.

Installed-channel comparisons use three complete trees: `shipping-current`, frozen from commit `3621d40db11bd2dfd89192bf7270b4a50603293a`; `concise`, with a short task-routing entrypoint and complete references; and `expanded-initial`, the archived initial canonical skill evaluated under the `package` label. The current canonical skill is a later follow-up candidate. The earlier `baseline` tree remains historical. CLI-discovered guidance comes from the evaluated binary and cannot compare installed variants.

## Provenance and execution conditions

The current cohort reports identify the frozen evaluated CLI by SHA-256:

```text
8c1de8264dc2124deba1a163e2852e2a663a2c4603e55d2b451dc27339f20eae
```

Their checkout provenance is `3621d40db11bd2dfd89192bf7270b4a50603293a`, with source edits present in the working tree. The checkout commit therefore identifies the base, not every evaluated source byte. CLI and installed-skill hashes identify the actual evaluated artifacts. `MONOCHANGE_EVAL_CLI_PATH` pins the preserved executable; neither agents nor graders silently fall back to a released binary on PATH.

Fixture and grading copies isolate Git roots and saved artifacts, but agents were not confined to those filesystems. A concise-skill Poetry trial read ancestor project source. That source tree changed between runs while the evaluated CLI remained frozen; skill and binary hashes do not identify every source byte consulted. These are exploratory field comparisons, not a clean causal experiment isolating skill text.

The reports record runtime version `2.1.281`. The requested `sonnet` alias resolved to `claude-sonnet-5`; interpretation must use the recorded effective model rather than assume what an alias means. An initial authentication failure was infrastructure failure, not evidence against a skill. The runtime's authentication helper needed its operating-system PATH entries available.

The harness executes cells sequentially within a process, but this experiment launched **eight external concurrent cohorts** and rotated variant orders. Shared CPU contention affects recorded duration; these timings cannot support a clean speed ranking. Each report records requested/effective model, runtime, hashes, usage when available, and execution outcomes. The first-pass reconciliation below preserves the original recorded usage and cost; grading adds no agent calls.

## First-pass comparison

Final hardened isolated regrading of all **114 original agent trials** gives **shipping-current 36/38, concise 36/38, expanded-initial 34/38**. Each cell is one trial. The original runtime recorded **966,351 input plus output tokens, excluding cache tokens, and $63.81** across these trials. These are recorded experiment totals, not a cost projection. [Compact JSON evidence](reports/first-pass.json) preserves all outcomes, failed check ids, artifact hashes, original usage, and metadata provenance without committing raw transcripts.

The full hardened replay initially scored one shipping readiness trial incorrectly because it invoked the pinned CLI through an explicitly assigned `MONOCHANGE_BIN` variable. Manual transcript review confirmed the actual readiness command and blocked result. Command evidence now normalizes that exact literal binding within the same submitted Bash tool call, while retaining raw transcripts and rejecting unbound or mismatched executable variables. A focused saved-artifact replay passes. The compact record preserves both the full replay and this one-cell corrective grade; no new agent call or duplicated cost was added.

A separate [normalization audit](reports/command-normalization.json) inspected submitted command inputs across all 141 saved trials. Eight inputs normalized across four trials; readiness was the only changed check outcome. All 69 command-scoped checks passed with the final recognizer. Isolated full-check replay of the three other affected trials preserved their previous outcomes, including the genuine concise prefix failure. Original artifacts, pinned binaries, and the full replay records remain preserved.

The original reports requested `sonnet`; runtime initialization recorded `claude-sonnet-5`. Legacy saved execution metadata omitted the original request, so replay correctly reports that field unavailable. The JSON distinguishes the request recovered from original reports from the unavailable replay field. All trials used runtime `2.1.281` and the frozen CLI hash above.

| Variant          | Skill SHA-256                                                      |
| ---------------- | ------------------------------------------------------------------ |
| shipping-current | `85175ae1715bbc37ecebbedc88b686cad07483bf72de1e08b884320fe851b833` |
| concise          | `964923c4bde9ffe12e8d095682d8212acce743d90a4330217e728af19b3ed0f3` |
| expanded-initial | `014e182651bc02715848f745355e3e759862d5473c8eec919db27cafff0f71b4` |

| Scenario                                  | Shipping | Concise | Expanded initial |
| ----------------------------------------- | -------- | ------- | ---------------- |
| `adopt-mixed-monorepo`                    | pass     | pass    | pass             |
| `adoption-disabled-ecosystem`             | pass     | pass    | pass             |
| `adoption-gitea-host-config`              | pass     | pass    | pass             |
| `adoption-npm-private-root`               | pass     | fail    | fail             |
| `adoption-populate-custom-workflow`       | pass     | pass    | pass             |
| `adoption-roots-and-exclusions`           | fail     | pass    | pass             |
| `adoption-rust-independent`               | pass     | pass    | pass             |
| `adoption-update-added-package`           | pass     | pass    | pass             |
| `breaking-change-changeset`               | pass     | pass    | pass             |
| `dart-workspace-root-package`             | pass     | pass    | pass             |
| `deno-adoption-and-version-write`         | pass     | pass    | pass             |
| `dependency-propagation`                  | pass     | pass    | pass             |
| `ecosystem-deno-jsonc-version`            | pass     | pass    | pass             |
| `ecosystem-flutter-type-alias`            | pass     | pass    | pass             |
| `ecosystem-go-no-tags-baseline`           | pass     | pass    | pass             |
| `ecosystem-python-poetry-version`         | fail     | pass    | fail             |
| `ecosystem-same-name-distinct-owners`     | pass     | pass    | pass             |
| `gitignore-release-state`                 | pass     | pass    | pass             |
| `go-tag-versioning`                       | pass     | pass    | fail             |
| `guardrail-config-repair`                 | pass     | pass    | pass             |
| `guardrail-disabled-ecosystem-recovery`   | pass     | pass    | pass             |
| `guardrail-private-package-lint-repair`   | pass     | pass    | pass             |
| `guardrail-unknown-target-repair`         | pass     | pass    | pass             |
| `planning-dependency-prefix-precedence`   | pass     | fail    | fail             |
| `planning-first-stable-release`           | pass     | pass    | pass             |
| `planning-fixed-nightly-base`             | pass     | pass    | pass             |
| `planning-group-changelog-filter`         | pass     | pass    | pass             |
| `planning-group-highest-baseline`         | pass     | pass    | pass             |
| `planning-named-customer-artifact`        | pass     | pass    | pass             |
| `planning-prerelease-preview-idempotence` | pass     | pass    | pass             |
| `planning-regex-glob-version-files`       | pass     | pass    | pass             |
| `planning-structured-version-files`       | pass     | pass    | pass             |
| `planning-workflow-input-forwarding`      | pass     | pass    | pass             |
| `publish-readiness-gates`                 | pass     | pass    | pass             |
| `python-release-version-write`            | pass     | pass    | pass             |
| `release-title-configuration`             | pass     | pass    | pass             |
| `release-without-publishing`              | pass     | pass    | pass             |
| `stream-split-changesets`                 | pass     | pass    | pass             |

Failures have distinct causes:

- **Private npm root:** concise and expanded initial failed `only-public-released`, proposing public versions `1.3.0` and `2.0.0` instead of the requested `1.2.4`. The agents changed source/API while addressing manifest lint/export metadata; classification escalated the bump. This is a scope failure, not a harmless choice of package ids.
- **Ownership filters:** shipping failed `automatic-ownership-filter-recorded`. It did not record the requested automatic registration filter; raw discovery and explicit ownership need separate verification.
- **Poetry own version:** shipping and expanded initial failed `prepare-writes-poetry-version-and-preserves-metadata`. The computed release target was plausible, but the actual Poetry version field did not update because explicit own-version selection was missing. The concise pass involved ancestor-source inspection, which limits attribution to its skill text.
- **Go tag identity:** expanded initial failed `a-release-is-actually-planned`, proposing `github.com/acme/core/v1.3.0` instead of the existing `core/v1.3.0` namespace. Arbitrary package ids are accepted; silently changing release identity and hiding lost history through `initial_version` is not. Source review also found custom tag templates incorrectly resolved baselines through `v`; the final source suite and contracts verify that correction separately from the frozen agent trials.
- **Dependency prefixes:** concise and expanded initial failed `dependency-prefixes-and-field-scope`, leaving a native dependency at `^2.4.0` instead of `=2.4.0`. Ecosystem prefix settings apply to typed entries, so automatic native synchronization requires an explicit selected-manifest override.

The first pass does not establish a winning skill: shipping and concise tie, failures differ by task, and mutable ancestor source plus single trials prevent a clean causal comparison. Grader fixes were audited separately and results were regenerated from preserved agent artifacts.

## Follow-up comparison

Hardened regrading of all **27 repeated agent trials** gives **24/27 passing: shipping-current 9/9, concise 8/9, and expanded-revised 7/9**. These are three divergent tasks with three trials per task/variant, not another complete 38-task matrix. Original recorded usage totals **448,949 input plus output tokens excluding cache tokens, and $28.92**. [Compact follow-up evidence](reports/follow-up.json) preserves original and hardened grades separately, together with usage and provenance. The evaluated revised canonical candidate is frozen at `b482fd934ac2e94bba3f5c69ba8ea15de0163fcf77ab5babc3ac9bfc8c160f19`; shipping-current and concise retain their original bytes.

| Task                                    | Shipping | Concise | Expanded revised |
| --------------------------------------- | -------- | ------- | ---------------- |
| `adoption-roots-and-exclusions`         | 3/3      | 3/3     | 3/3              |
| `ecosystem-python-poetry-version`       | 3/3      | 3/3     | 3/3              |
| `planning-dependency-prefix-precedence` | 3/3      | 2/3     | 1/3              |

The exact revised candidate is preserved in [skill-variants/expanded-revised](skill-variants/expanded-revised), even though its original trial label was `package`. The earlier [expanded-initial](skill-variants/expanded-initial) tree preserves the distinct `014e182651bc02715848f745355e3e759862d5473c8eec919db27cafff0f71b4` first-pass candidate. Reproducing either experiment requires its archived tree and recorded binary; selecting live `package` after subsequent edits does not reproduce the same bytes.

After the trials finished, “no reachable tag” became “no matching repository tag” in the canonical skill. This aligns its wording with the actual tag-selection policy without changing CLI behavior. The final canonical SHA-256 is `ae27c7c3390d27ed7567582a29f642532b845fd806d1fb7234833ebb80f3dc48`, which differs from the evaluated `b482…` candidate. It did not receive another full agent matrix for that small factual correction; the archive preserves exactly what was evaluated.

The ownership-filter, Poetry, and prefix cohorts each finished nine trials. Original and replay metadata retain requested `sonnet`, effective `claude-sonnet-5`, runtime `2.1.281`, checkout `896a5470208b08627689418cfc55354d3db6c66f`, and CLI SHA-256 `00998ccd5b68fdd97aa85cfd49daa48177d0f949b433c8542b146349c57352cf`. Hardened replay used the same frozen CLI; it made no additional agent calls.

Prefix failures in canonical repeats 1 and 3 are substantive: each agent read the revised configuration reference but configured only the deployment override, leaving the native dependency at `^2.4.0`. Concise repeat 3 also failed the prefix outcome. All three fail `dependency-prefixes-and-field-scope` after hardened replay. Shipping repeat 2 and concise repeat 1 originally failed the safety pattern because bare read-only `git tag` matched tag creation; both pass the corrected check. Several prefix agents inspected ancestor source, reinforcing the filesystem confounding described above.

The repeated ownership and Poetry successes across every variant weaken any claim that the first-pass differences came solely from the entrypoint. The expanded revision still failed two of three prefix trials despite explicit reference guidance. This focused follow-up supports keeping concrete field examples and planned-write verification, but does not establish that a longer or newer skill performs better overall.

Together, the two formal comparisons contain **141 original agent invocations**, with **1,415,300 input plus output tokens excluding cache tokens and $92.73 recorded cost**. Regrading reuses those invocations; its reports do not add new model usage. Historical/pilot runs are outside these totals.

Source changes continued after freezing the first-pass CLI, including the shared JSON replacement and custom-tag baseline corrections. Reports against frozen binaries cannot validate later code. Regrading records original agent provenance separately from the current grading binary and is not a new agent trial.

## Final source verification

The results below distinguish the completed local verification phase at source commit `937cea83f537d3ae6d61e0f89de9a2e54cdee11c` from verification after PR CI found a Rustdoc classifier defect and release-intent mismatches. Source commit `46990b74ceb81155e066996c47f8f234e50ef07b` corrects them. The frozen skills and all 141 paid-trial records retain their original provenance.

The completed pre-CI executable has SHA-256 `a59c7657c9a2f28aab4303760ac0f3cb6ed4f5c042e7392feaa748f4e661d13f`, built from source commit `937cea83f537d3ae6d61e0f89de9a2e54cdee11c`. It passed **22/22 agent-free contract scenarios and 106/106 checks**. [Preserved pre-CI contract evidence](reports/contracts-before-ci-fix.json) records every scenario, check id, result, and binary/source provenance.

The pre-CI Node suite passed **144 tests across seven files**, including 58 harness tests. The pre-CI Rust aggregate passed **3,928/3,928 tests**, with none skipped and no orphan snapshots; all 3,928 instrumented tests also passed. Rust line coverage in that phase was **97.03% overall**, and patch coverage was **498/498 executable changed lines (100%) across 14 Rust files**, with every changed file at 100%. Build, lint, documentation tests/synchronization, project changeset validation, dry-run preparation, and affected-path verification also passed for that phase. Required PR checks remain the merge gate. Local `test:all` includes the contract suite using the caller's existing Git identity and GPG signing setup; required hosted CI runs harness unit tests and Rust fixture regressions without scheduling those signed-fixture contracts.

The post-CI rebuilt executable has SHA-256 `9406d48fa3befe8d242d35f6062177f06a68885d3a43e5ebf4c410ff1a1699f8`, containing source commit `46990b74ceb81155e066996c47f8f234e50ef07b`. At checkout `b05f2b2ef4a8f2c490e91798b069d07403a74c83`, it passed **22/22 contracts and 106/106 checks**, with zero agent calls. [Current contract evidence](reports/contracts.json) preserves this phase separately. Build and all 144 Node tests passed again, and comparison-based `affected-packages --from origin/main --verify` passed for 11 changed packages. Neither contract phase reruns the paid agent matrix or all 315 authored checks.

The first post-CI full run passed all **3,957 nextest cases**, but its aggregate command exited 1 at the cargo-insta gate because one classification snapshot still included the removed Rustdoc attribute. Classification verdicts and assertions were unchanged. The first instrumented coverage attempt exited 101 on that same pending snapshot; neither attempt established a passing aggregate gate.

`snapshot:update` accepted the single expected snapshot change, committed as `36132b4cc`; production code remains unchanged from `46990b74`. That update run executed 3,957 cases, with 3,952 passing and five GitHub mock-client build failures reported as `Other`. A subsequent certificate probe found 161 native roots in 291.7 ms versus 121 roots from the Nix certificate bundle in 25.2 ms, with no probe errors. This suggests macOS certificate-discovery pressure, but the original OS error was not retained and the exact cause is unconfirmed. The Rust test scripts now fall back to the Nix certificate bundle when `SSL_CERT_FILE` is unset or empty, preserving an explicit nonempty setting; production TLS behavior is unchanged. The CLI contract evidence and all paid-trial artifacts remain unchanged.

Fresh verification at source `36132b4cc` and upper checkout `d98d927097e98a5ad57dffa0cd238744501f4b27` then passed the complete `test:cargo` gate: **3,957/3,957 tests, none skipped, no pending or unreferenced snapshots**. `coverage:all` also exited 0 with **all 3,957 instrumented tests passing**, none failed or ignored, and **97.03% overall Rust line coverage**. `coverage:patch` exited 0 with **530/530 executable changed Rust lines covered (100%) across 15 files**, every file at 100%. Fresh documentation tests passed. Local logs are `.monochange/local/cargo-shipping-corrected.log`, `coverage-shipping-corrected.log`, `patch-shipping-corrected.log`, and `docs-shipping-corrected.log`; `patch-shipping-corrected-by-file.json` preserves per-file counts. These coverage figures apply to Rust and make no TypeScript coverage claim. No additional paid agent calls were made. Required PR checks remain the merge gate.

## CLI defects and guidance corrections in this expansion

These changes are supported by reproduced behavior, focused source tests/fixtures, and the verification phases above, including the corrected 3,957-test aggregate and 100% executable Rust patch coverage. Required PR checks remain the merge gate.

- **Skill installation followed symbolic links.** Existing linked documents, subdirectories, and dangling links could redirect writes outside the chosen skill tree, including forced updates. Installation now checks the destination and bundled paths before writing and reports offending links. Parent-directory links above the explicitly chosen destination retain their supported behavior.
- **Initialization could create unusable ownership.** Multiple ecosystem manifests in one directory could produce duplicate path owners and a starter that failed immediately. Initialization now diagnoses that conflict before writing config or workflows. Distinct-directory name collisions also receive unique deterministic ids, including three-or-more collisions and names that already resemble generated suffixes.
- **Initializer/population help promised workflows that do not exist.** The current default workflow set is empty. Help/output now state that `populate` preserves the existing config and adds nothing; `create`, `preview`, and `prepare` work directly, while custom `[cli.*]` workflows must be authored explicitly.
- **Invalid automatic-discovery globs were silently ineffective.** Include and exclude patterns now fail with the ecosystem, field, and offending pattern. Validation includes excludes even when the include list is empty.
- **Deno JSONC support was inconsistent across the release path.** Validation could demand `deno.json` despite a discovered `deno.jsonc`, and synchronization could lose comments/layout. Supported JSONC now follows initialization, validation, discovery, and preparation. Malformed block comments and comment-split tokens are rejected rather than silently joined or accepted. Deno documents `deno.jsonc` support for comments and trailing commas in its [configuration guide](https://docs.deno.com/runtime/fundamentals/configuration/).
- **Python dependent identity and Poetry writing were incomplete.** Canonical aliases now connect native names such as `PY_Core` to declared `py-core`/`py_core` constraints for propagation and manifest updates. An additional alias correction resolves configured ids during selected-field updates instead of comparing them directly to native dependency keys; focused fixtures cover selected-field and automatic synchronization behavior. Poetry-only versions and runtime/group dependencies update their existing tables while preserving extras, markers, comments, and source metadata. PEP 621 precedence and dynamic versions remain relevant boundaries.
- **Adapter aliases could create dependency edges across ecosystems.** Final review found that a Python canonical alias such as `foo-bar` could also match a Cargo dependency with the same spelling and create an unintended Python edge. Shared dependency-edge materialization now scopes adapter-provided aliases to consumers in the producer's ecosystem. Exact native-name matching retains its existing behavior. Focused source tests cover alias collisions, same-ecosystem matching, and deduplication; this correction is separate from configured-id resolution in selected versioned fields.
- **The inferred Poetry command used a removed option.** `poetry lock --no-update` fails with Poetry 2; `poetry lock` preserves existing pins by default. The adapter and guidance now use that command; older installations can configure an explicit override. Poetry's current [lock command reference](https://python-poetry.org/docs/cli/#lock) documents that default and the separate `--regenerate` option for rebuilding locked versions.
- **Explicit versioned-file rules could be overwritten by native synchronization.** Preparation now applies selected rules after native updates and before lock commands. Dependency-only npm entries preserve their own root version, scalar selections use matching constraints, and configured dependency ids resolve to native names. Previews must show both the selected key and its final prefix/version.
- **JSON field selections could schedule duplicate edits.** Selecting root `version`, repeating a field, or combining a dependency object with an exact dependency field could apply the same old byte span more than once. Length-changing replacements then corrupted JSON. The shared edit writer now coalesces identical replacements and rejects conflicting/overlapping edits before applying them; grow/shrink fixture tests cover these cases. This fix postdates the frozen cohort binary; final source tests verify it separately.
- **Custom tag baselines did not follow the configured format.** Template-rendered tags were resolved through a generic `v` prefix, and splitting at the final `v` could misread a prerelease such as `dev.7`. Matching now round-trips configured tag formats and chooses the highest matching SemVer; previous-release lookup uses the same identity. Docs clarify that repository tags are considered without a branch-reachability filter. Custom and prerelease-tag fixture checks pin this contract; final source verification is separate from the frozen agent trials.
- **Release titles were validated against the wrong context.** Valid release-context fields such as `previous_version` could be rejected while unrelated version-value variables passed validation. Title validation now uses the release renderer's context at package, group, and default scope, accepts valid Jinja expressions, and rejects unsupported variables or malformed syntax. The final source suite verifies the supported contexts and rejection paths.
- **Analysis snapshots omitted native control manifests.** Materialized Git/staged snapshots dropped `go.mod`, `pyproject.toml`, and GitHub Actions control files needed to validate mixed-workspace configuration. Snapshot filtering now retains these files. This enables configuration validation during analysis of supported packages; it does not add semantic analyzers for Go, Python, or GitHub Actions. The final source suite verifies snapshot retention and configuration behavior.

## Findings from PR CI

- **Rustdoc attributes changed API signatures.** Documentation text embedded in parsed Rust items could appear in rendered API signatures, making a documentation edit look like a breaking API change. The Cargo analyzer now removes Rustdoc attributes when deriving signatures while preserving attributes with API significance in collectors that include item attributes. Focused regressions cover documentation-only edits and genuine attribute changes. Existing standalone-function and re-export collectors omit item attributes, so this does not establish complete semantic attribute analysis. This fix was discovered after the pre-CI phase and is verified by the corrected aggregate above.
- **Additive APIs require minor release notes.** CI also found two compatible core API additions (a dependency-identity constant and a selected-field writer) and one Deno API addition that had patch changesets. The core entries in `canonical-dependency-identities.md` and `versioned-file-dependency-rules.md`, and the Deno entry in `deno-jsonc-release-preparation.md`, now use `minor`. Config retains its patch note; it adds a MiniJinja dependency without a new public API. The Cargo classifier correction has its own patch note. Explicit changed-path checks had verified package coverage without checking API alignment. Agent workflow guidance now requires committed-code verification with `monochange step affected-packages --from origin/main --verify`, matching the comparison-based CI gate.

The Rust test-layout guide now matches the repository's `__tests__/` module-reference rules and designated integration-test crate. Workflow guidance also makes `no-changeset-required` a human-only decision.

The skill/docs were also corrected around raw discovery versus registered release ownership, existing-config updates, no-config validation success, cross-stream validation limits, JSON quiet/filter behavior, preparation consuming changesets, Python/Go version ownership, prerelease state, section identifiers versus headings, valid stream/output ids, explicit dependency selection, and group changelog filtering versus complete hosted/named notes. The concise candidate changes how this guidance is routed, not the repository's authorization boundaries.

## Historical rounds

The previous findings document reported an initial eight-scenario round followed by a 29-scenario matrix. It recorded one model and one run per cell, approximately 143k tokens and about $10 of agent time across its recorded runs. Those figures are **historical**, are not current experiment totals, and are not newly validated by this expansion. Older saved records without execution/provenance fields fail the new replay requirements and need fresh runs.

Historical CLI fixes included scoped npm TOML key quoting during `init`, configured Python release loading, exact Go module-path dependency matching without rewriting unrelated third-party modules, true changeset dry runs, quoted pnpm and Dart dependency-key handling, and preview-cache invalidation after changeset content edits. The earlier document contained a contradictory passage calling the Go rewrite unresolved; that described the pre-fix observation. Its resolved behavior is now represented by `go-dependent-require-rewrite`, and final-build contract results determine current status.

Historical skill corrections replaced stale command spellings, documented `versions list|sync`, separated preview `compatibility_evidence` from classification `decision.compatibility_impact`, required explicit Python own-version field selection, and identified preview as the cross-stream gate. Those remain useful regression targets; their historical success does not prove the current full suite passed.

Earlier grader lessons still apply: consumed changesets are not missing work, group ids are choices rather than fixed literals, a mention of publication is not an executed upload, earlier mutating checks change later inputs, and severity expectations must agree with the toolchain-dependent evidence rather than silently override it.

## Remaining limits

- No registry uploads, live provider changes, remote tags, releases, release-PR merges, or manual release/publish workflow triggers were performed. Local readiness/lifecycle evidence does not prove production publication success.
- Fixtures are small synthetic repositories. Long histories, large dependency graphs, production performance, unavailable package managers, and every configuration combination remain outside this matrix.
- Semantic analysis depends on available compilers, dependencies, and analyzer coverage. Release-intent checks do not prove complete semantic compatibility analysis.
- Single-run outcomes and contended durations cannot establish a precise variant ranking. Repeats, intended-skill consultation, and stable artifact provenance matter.
- Harness unit tests and isolated replay reduce false passes but do not prove that every scenario expectation or grader is correct. Grader changes must be audited independently of their pass rates.
- Required PR checks remain the merge gate. Passing source tests/contracts, 100% patch coverage, and reviewed comparisons do not prove every unexercised configuration or production release path is correct.
