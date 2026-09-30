# Design: monochange skill evaluations

## Purpose and scope

Measure whether an agent using monochange guidance completes realistic release-planning work, and separately pin the CLI behavior that guidance relies on. The committed matrix contains 60 scenarios: 38 agent tasks and 22 agent-free contracts, with 315 checks across 23 fixture families. These comprise 22 primary fixtures and a supplemental Go tag-baseline family copied by two contract checks. [COVERAGE.md](COVERAGE.md) lists every scenario and its precise surface; [FINDINGS.md](FINDINGS.md) records executed evidence rather than authored coverage.

Agent tasks span adoption, configuration updates, six package ecosystems, release intent, groups and propagation, prereleases, versioned files, audience outputs, diagnostic recovery, and publication boundaries. Contracts cover mechanical command behavior and safety gates without model calls. Contract passes establish binary behavior; they do not establish which skill variant produces better agent outcomes.

## Scenario and grading design

Definitions live in `scenarios/*.json`; the schema is maintained in `lib/types.ts`:

- `id`, `title`, `fixture`, `prompt`, and `expectation` identify the task and intended outcome.
- `setupFiles` and `setupCommands` prepare the copied fixture before execution.
- `checks` assert command results, files, absent files, or agent-authored transcript content.
- `agent = false` selects deterministic contract execution.
- `skillSource` can pin `installed` or `cli`; `variants` supplies optional prompt wording selected by `--instruction-variant`.

Write prompts as realistic user requests with explicit outcomes and side-effect boundaries. Provide fixtures and observable requirements, rather than suspected defects or an intended implementation. Checks should accept valid alternative configuration and naming choices. Prefer CLI or file properties to transcript wording; transcript checks exclude skill text and tool responses and retain assistant text and tool inputs.

Each fixture gets an isolated Git root before setup and agent work. Initial commits are signed; scenarios that define their own history own their initial commit. Fixtures are committed without `.git`. `${MONOCHANGE_ROOT}` placeholders resolve to the current checkout when copying.

## Isolation and trustworthy replay

`run.ts` orchestrates selection, setup, agent execution, grading, and reports. Supporting modules separate options, paths, repository setup, runtime handling, process lifecycle, provenance, and grading. Their tests run under `test:node`.

Workspaces live in `.work/runs/<scenario>__<variant>__source-<channel>__<repeat>`; instruction variants add a separate identity component. Installed guidance is copied into `.claude/skills/monochange`. CLI guidance installs nothing and comes from the evaluated binary. Project-only settings and strict MCP configuration exclude personal runtime settings and configured MCP servers, while built-in runtime skills can remain available. Inspect transcripts to confirm the intended guidance was consulted. Repository policy can still apply through parent instructions; scenarios must respect the project's publication prohibitions.

Every grading pass uses a separate `.work/grades/` copy. Mutating checks can consume changesets or prepare releases there without altering saved agent artifacts. Checks within one grading pass remain sequential and share that grading copy, so later checks must account for earlier mutations.

These copies isolate Git and saved artifacts, not filesystem access. Agents can read ancestor source and did so in the concise Poetry trial. The parent source changed between frozen-CLI runs. Binary and skill hashes do not identify every external source byte consulted, so comparisons are exploratory field evidence rather than a clean causal experiment isolating skill text.

The harness resolves `target/debug/monochange` without falling back to `PATH`. `MONOCHANGE_EVAL_CLI_PATH` can pin a preserved build snapshot; the file must exist and retain the executable name `monochange`. The selected executable's directory is prepended to child `PATH` for agents, setup, and checks. Keep binary and skill bytes fixed during a comparison.

Reports record checkout commit, CLI SHA-256, installed-skill SHA-256 when applicable, requested/effective model, runtime version, durations, and usage. Checkout identity alone does not capture uncommitted source; executable and skill hashes identify evaluated artifacts. Runtime crashes, authentication failures, timeouts, nonzero exits, and missing successful terminal results fail execution even when partial artifacts satisfy checks. POSIX timeouts terminate the runtime process group and tool subprocesses.

`--regrade` loads saved artifacts, transcript, execution outcome, and original provenance without calling an agent. Missing or invalid saved execution data fails closed. Reports distinguish original agent provenance from the current grading checkout/binary and preserve recorded cost and duration. Rerun changed tasks or legacy runs missing provenance instead of treating them as new evidence.

## Skill comparisons and interpretation

Select variants explicitly to control cost. `package` is the canonical live skill; `shipping-current` freezes the shipping version at `3621d40db11bd2dfd89192bf7270b4a50603293a`; `concise` tests a shorter task-routing entrypoint with complete references; `expanded-initial` archives the initial canonical skill evaluated under the `package` label; `baseline` preserves the earlier experiment. Frozen trees remain unchanged. Installed-channel agent runs compare these trees; CLI-channel runs compare discovery/use of the bundled guidance, not installed variants.

Runs execute sequentially within a harness process; the first comparison used eight externally concurrent cohorts with rotated variant order, so recorded durations are contended. Its 114 single-trial cells and usage are recorded in [reports/first-pass.json](reports/first-pass.json). Repeat divergent or uncertain scenario/variant cells before claiming improvement, and keep requested model, channel, prompt wording, toolchain, and binary stable. Separate agent failures from grader defects and runtime failures. A single successful run is useful regression evidence, not a statistically reliable ranking or proof that no hidden bugs remain.

The suite excludes production registry uploads, remote tag changes, hosted-provider mutations, release merges, and release/publish workflow triggers. Small synthetic fixtures do not measure large repositories, long production histories, every configuration combination, performance limits, every lockfile format, or complete semantic analyzer coverage. Rust tests provide additional implementation evidence. Extend coverage according to observed failures and the gaps in COVERAGE.md rather than increasing the scenario count alone.

## Useful grading invariants

- `prepare` consumes applied changesets; grade resulting versions and release files, not their continued presence.
- Ungrouped plan versions live in `release_targets`; top-level `version` can be null.
- Pre-1.0 bump rules shift major to minor and minor to patch unless explicit release intent overrides them.
- `step validate` can succeed without a config and does not reject cross-stream files; inspect presence and preview a real pending release.
- `discover` inventories raw packages; `config` exposes registered release ownership.
- JSON assertions should parse the complete payload with Node. The CLI's restricted `--jq` filter is not a full jq replacement.

See [README.md](README.md) for commands, options, prerequisites, and report locations.
