# monochange skill evaluations

Evaluations for the `monochange` agent skill (`packages/monochange__skill`).

The skill's job is to let an agent do real release planning in a multi-ecosystem monorepo without getting lost: adopting monochange, authoring changesets that match the actual change, configuring versioned files and groups, previewing versions, and gating publishing. Every scenario here exercises one of those decisions against the real `monochange` CLI, and grades the artifacts the agent leaves behind rather than its prose.

## Why the checks are artifact-based

An agent that explains release planning beautifully but writes a changeset with the wrong target or bump has failed the task. So each scenario runs a real change in a throwaway monorepo and then grades with the same tools CI would:

- `monochange step validate` for config and changeset shape,
- `monochange check` for lint rules and changeset coverage,
- `monochange preview --format json` for planned version bumps,
- `monochange discover --format json` for the package map,
- targeted file and transcript assertions for the parts a command cannot see.

## Layout

```
evals/monochange-skill/
├── run.ts                 CLI runner: selects scenarios, drives the agent, reports
├── lib/
│   ├── agent.ts           headless agent invocation and transcript capture
│   ├── grade.ts           check execution
│   ├── paths.ts           layout, fixture copying, `monochange` CLI resolution
│   └── types.ts           scenario and result types
├── fixtures/              throwaway monorepos the agent modifies
├── scenarios/             one JSON file per evaluation
├── skill-variants/        complete `monochange` skills to compare
├── results/               transcripts and reports (git-ignored)
└── DESIGN.md              design notes and grading philosophy
```

## Running

Node 24 executes TypeScript directly through type stripping, so there is no loader and no `tsx` dependency. Run from the repository root:

```sh
devenv shell eval:skill --list
devenv shell eval:skill --all --variant package
devenv shell eval:skill --scenario breaking-change-changeset --variant package
devenv shell eval:skill --all --variant shipping-current --variant concise --variant package --repeats 3
devenv shell eval:skill:contract
```

Useful flags: `--model` (or `MONOCHANGE_EVAL_MODEL`), `--agent-bin` (or `MONOCHANGE_EVAL_AGENT_BIN`), `--timeout`, `--repeats`, `--instruction-variant`, `--skill-source`, `--contract-only`, `--list`, `--regrade`. `--instruction-variant` swaps in the wording from a scenario's `variants` map. `--all` is the default; repeated `--scenario` flags select a cohort. Misspelled flags and scenario IDs, nonpositive repeat counts, and empty selections fail before any agent runs. A failed suite exits nonzero.

`--skill-source installed` (the default) copies the variant into the run workdir as a project skill. `--skill-source cli` installs nothing: the agent starts with only the toolkit and has to find the guidance through the CLI, which is the path `monochange skill` serves. A scenario can pin the channel with a top-level `"skillSource": "cli"`. Runs are keyed by variant plus channel, so `package` and `package-cli` never overwrite each other.

### Prerequisites

1. Build the CLI the fixtures are graded with. The harness resolves `<repo>/target/debug/monochange` and never falls back to `PATH`, because a released `monochange` may have a different command surface and grading against it measures nothing:

   ```sh
   devenv shell -- cargo build -p monochange
   ```

2. The agent runtime must be authenticated. Runs use `--setting-sources project` and `--strict-mcp-config` to exclude user settings and configured MCP servers. Use `--agent-bin /absolute/path/to/claude` when the runtime is outside the development shell's `PATH`. Its authentication helpers must also be reachable; on macOS this includes `/usr/bin/security`. A runtime authentication failure is recorded as a failed execution, never scored as skill evidence.

Set `MONOCHANGE_EVAL_CLI_PATH` to an existing snapshot of the built executable when other work may rebuild `target/debug/monochange` during a comparison. The snapshot must retain the filename `monochange`, for example `/tmp/pinned-cli/monochange`, because agents and command checks resolve it through that directory. Invalid snapshot paths fail without falling back to `PATH`, and reports hash the selected executable.

Reports record the checkout commit, executable and installed skill SHA-256 hashes, requested and effective model, runtime version, durations, and reported usage. Agent-free contracts test the binary and do not establish which skill works better. CLI-discovered guidance always comes from the evaluated binary, so that channel cannot compare installed skill variants.

The runner executes its cells sequentially. Cohorts launched concurrently by an external orchestrator share machine resources and can contend for runtime capacity, so their recorded durations cannot establish a fair latency comparison between skills. Compare outcomes across those cohorts; measure latency with an otherwise idle machine and consistent scheduling.

Harness unit tests and Rust fixture regressions are durable gates in the required CI test job. Local `test:all` also runs all 22 agent-free contracts. Those contracts create signed fixture commits using the caller's existing Git identity and GPG setup, so they are not scheduled in hosted CI without that configuration. These gates do not call a paid agent runtime. Paid skill comparisons remain explicit evaluation runs; their completed evidence is recorded in [FINDINGS.md](FINDINGS.md) and compact JSON under `reports/`.

## How isolation works

Each run copies a fixture into `.work/runs/<scenario>__<variant>__source-<channel>__<n>` and installs the chosen variant as `<workdir>/.claude/skills/monochange`. Instruction variants add an `__instruction-<id>` component. The project copy is the skill under test. Built-in runtime skills can still be present; inspect transcripts to confirm the intended skill was consulted.

Every fixture gets its own Git repository before scenario setup, preventing Git commands from discovering the parent checkout. The harness signs a baseline commit with the user's existing identity unless the scenario's setup commands own their initial commit and history. It does not override Git identity or signing configuration.

Checks execute against a separate `.work/grades/` copy. In-workspace symlinks are remapped to that copy; escaping or broken links fail before grading rather than pointing back to saved artifacts or external files. A crash, timeout, runtime error, missing terminal result, or result without explicit runtime success status fails even if some artifacts happen to satisfy checks. POSIX timeouts terminate the agent process group, including its tool subprocesses.

The workspace `monochange` binary is prepended to the child's `PATH`, so `monochange ...` inside the fixture resolves to the build under test.

**Inherited repository policy reaches the agent.** Run workdirs live under this repository, so an agent working in one can read the monochange `AGENTS.md` that sits above `.work/`. That file forbids publishing packages and using registry credentials, which is the correct behavior to encode in a scenario rather than fight: a prompt that asks an agent to publish will get a refusal that is right about the policy even when it looks wrong about the scenario. Write scenarios that grade the gate — did the agent consult readiness and report what it found — rather than grading an action the repository forbids.

Fixture and grading copies provide Git and artifact isolation, not filesystem confinement. Agents can read ancestor project source; a concise-skill Poetry trial did so. The parent source tree also changed between runs while the CLI snapshot stayed frozen. Artifact hashes make the evaluated skill and binary identifiable, but they do not identify every external source byte an agent consulted. Treat these trials as exploratory field evidence, not a clean causal experiment isolating skill text.

## Scenario inventory

60 scenarios: 38 agent tasks and 22 agent-free contracts. `devenv shell eval:skill --list` prints the current set.

`COVERAGE.md` maps every CLI decision surface to the scenario that pins it and records what is deliberately not tested. Run that file before adding a scenario, both to avoid duplicating a check and to find the gaps it lists.

## Authoring a scenario

A scenario is a JSON file in `scenarios/`. The fields that matter:

- `prompt` — handed to the agent verbatim. Write it as a user would: state the goal and the constraint (real repo, don't break consumers), not the mechanism.
- `fixture` — the monorepo to modify. Use a fixture that already has the shape the task needs, because adoption and configuration tasks differ from changeset-authoring tasks.
- `expectation` — what a correct solution looks like, used in the report.
- `checks` — the graded assertions.
- `setupFiles` — paths written into the workdir before the agent starts.
- `setupCommands` — shell commands run there first, such as `git init` plus a commit. Fixtures are committed without `.git`; a scenario that needs history declares it here.

Check kinds:

| Kind         | Grades                                                          |
| ------------ | --------------------------------------------------------------- |
| `command`    | a shell command's exit status and output substrings             |
| `file`       | a file exists, and its contents match (or `absent` them)        |
| `absent`     | a path does not exist                                           |
| `transcript` | the agent's own text and tool inputs match (or avoid) a pattern |

Transcript checks look only at content the assistant authored. Tool results and skill files the agent read are excluded, so a check cannot accidentally match the skill's own documentation instead of the agent's work.

Use `"scope": "commands"` on a transcript check when grading submitted shell operations, including prohibitions on publishing, pushing, or tagging. This scope matches actual `Bash` command inputs and excludes assistant prose, tool descriptions, and document contents written through other tools. An explanation such as "I did not run cargo publish" therefore passes; a submitted `cargo publish` command fails. Live runs and regrades extract this evidence identically, and missing command evidence fails the check. The patterns still inspect command text; they do not parse shell execution semantics.

Release safety checks include first-class `monochange step` mutations and publishing aliases. They permit a supported `--dry-run` on the same command segment, so an unrelated preview after `;` cannot excuse an earlier mutation. Arbitrary configured workflow names, shell variables, nested quoting, and indirect scripts still require transcript review; these lexical checks are evidence checks, not a sandbox. Readiness consultation requires a submitted readiness command, rather than a statement that the gate was consulted.

Command grading also recognizes a literal assignment of the evaluated executable immediately followed by invoking that variable in the same Bash input, such as `CLI=/absolute/path/to/monochange; $CLI step publish-readiness`. The assigned path must exactly match the CLI selected for grading; bindings never transfer between tool calls. The raw transcript remains unchanged. Expressions, unbound variables, reassignment chains, and other wrappers remain outside this narrow normalization.

Prefer a `command` check over a `file` check whenever the CLI can answer the question; prefer an outcome over one spelling.

Fixtures may use the `${MONOCHANGE_ROOT}` placeholder in any text file. The harness rewrites it to the checkout root when copying, so a fixture can reference the local repository by path without committing an absolute path.

A scenario may also declare `setupFiles` (paths to write into the workdir before the agent starts) and `setupCommands` (shell commands to run there, such as `git init` plus an initial commit). Fixtures are committed without `.git`, because a fixture that needs history should say so in its scenario.

## Fixtures

| Fixture          | Shape                                                                          | Used for                      |
| ---------------- | ------------------------------------------------------------------------------ | ----------------------------- |
| `mixed-monorepo` | Cargo crates plus scoped npm packages, no config                               | adoption                      |
| `npm-monorepo`   | pnpm workspace, three packages in one version group, a `user` changelog stream | changesets, streams, releases |
| `rust-workspace` | two independently-versioned crates, one depending on the other                 | propagation, gitignore        |
| `python-uv`      | uv workspace, two `pyproject.toml` packages, no config                         | adoption, versioned files     |
| `go-modules`     | two Go modules in one repository                                               | discovery probes              |
| `dart-workspace` | pub workspace with two packages                                                | discovery probes              |

The adoption and ecosystem cohorts exercise Cargo, npm, Python, Go, Dart/Flutter, and Deno JSONC. Planning tasks cover groups, prereleases, explicit versions, dependency prefixes, custom command inputs, structured and regex version stamps, and audience-specific outputs. Guardrail tasks cover diagnostic recovery, invalid targets, private packages, initialization refusal, and skill installation/read/update boundaries.

## Adding a skill variant

Copy a complete skill into `skill-variants/<name>/` (`SKILL.md` plus `skills/` and `examples/`) and pass `--variant <name>`. Comparing two variants is how a skill change is shown to fix a failure rather than merely changing the text. Keep one variant frozen so recorded comparisons stay meaningful; `skill-variants/freeze-baseline.sh` re-freezes `baseline` from a git ref.

The available comparison trees have distinct roles:

| Variant            | Source                            | Role                                                                                                                                                                        |
| ------------------ | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `package`          | `packages/monochange__skill`      | Live canonical published artifact; its bytes can change after a comparison.                                                                                                 |
| `shipping-current` | `skill-variants/shipping-current` | Shipping skill frozen at checkout `3621d40db11bd2dfd89192bf7270b4a50603293a`.                                                                                               |
| `concise`          | `skill-variants/concise`          | Frozen short task-routing entrypoint with complete supporting references.                                                                                                   |
| `expanded-initial` | `skill-variants/expanded-initial` | Initial canonical candidate evaluated under the `package` label in the 114-trial matrix; SHA-256 `014e182651bc02715848f745355e3e759862d5473c8eec919db27cafff0f71b4`.        |
| `expanded-revised` | `skill-variants/expanded-revised` | Revised canonical candidate evaluated under the `package` label in the focused repeated trials; SHA-256 `b482fd934ac2e94bba3f5c69ba8ea15de0163fcf77ab5babc3ac9bfc8c160f19`. |
| `baseline`         | `skill-variants/baseline`         | Historical candidate from the earlier experiment.                                                                                                                           |

Use `expanded-revised` to reproduce the evaluated revised text, together with the recorded CLI snapshot. After all 27 repeated trials finished, the canonical skill received one factual wording correction: “no reachable tag” became “no matching repository tag”. Its resulting SHA-256 is `ae27c7c3390d27ed7567582a29f642532b845fd806d1fb7234833ebb80f3dc48`; the exact evaluated `b482…` candidate remains archived. This final wording correction did not receive an additional full agent matrix. A directory named `package` under `skill-variants/` would take precedence over the live canonical package.

Without `--variant`, the runner selects all available variants. Select variants explicitly to control model-call cost and keep comparisons intentional. Keep the skill trees and executable unchanged during a comparison; repeat uncertain or divergent cases before claiming a winner.

The completed formal comparisons record these outcomes:

| Comparison                         | Original agent trials | Shipping | Concise | Expanded candidate         |
| ---------------------------------- | --------------------- | -------- | ------- | -------------------------- |
| 38-task first pass                 | 114                   | 36/38    | 36/38   | 34/38 (`expanded-initial`) |
| Three focused tasks, three repeats | 27                    | 9/9      | 8/9     | 7/9 (`expanded-revised`)   |

The 141 original invocations recorded 1,415,300 input plus output tokens excluding cache tokens and $92.73. Regrades made no new agent calls. [FINDINGS.md](FINDINGS.md) and the [first-pass](reports/first-pass.json) and [follow-up](reports/follow-up.json) evidence describe corrected grader outcomes, exact artifacts, remaining failures, and why these exploratory results do not establish a causal winner. The [final-build contract evidence](reports/contracts.json) separately records 22 passing scenarios and 106 passing checks against the final executable; it does not rerun the paid agent matrix or all 315 authored checks.

## Re-grading without re-running the agent

Agents are slow and stochastic, so a grader fix should not cost another round of runs:

```sh
node evals/monochange-skill/run.ts --all --variant package --regrade
```

This re-applies the current checks to the saved workdirs and transcripts. Use it after correcting a check, and re-run the agent only when the scenario itself changed.

Regrading requires saved execution outcomes and provenance. Older transcripts from before those fields were recorded fail closed; rerun them rather than presenting them as newly validated evidence. A regrade reports the original duration and cost and the current grading binary separately. It reads the original requested model from saved execution metadata, ignoring a replay's `--model` option. When legacy metadata lacks that field, the requested model is unavailable; the effective model from the saved runtime initialization record remains available.

## What the checks must not do

Grading agent output is easy to get subtly wrong, and a wrong check is worse than no check because it looks like evidence. Three failure modes bit the pina suite this harness is ported from, all of them false negatives on _correct_ work:

- **Matching one spelling.** A correct solution may write `fix` instead of `patch`, or quote an id that does not need quoting. Grade the property, not the phrasing.
- **Matching the transcript for a file property.** A run that reports "no `.changeset/` entry left" contains that string. Assert the artifact, not the sentence.
- **Matching content the agent read.** Skill bodies and tool results arrive in the stream too. Transcript checks only see assistant-authored text and tool inputs, or the skill's own docs would satisfy them.

Prefer a `command` check whenever the CLI can answer the question, and prefer an outcome over a spelling.

### Traps specific to monochange

These bit this suite. All of them produce false negatives on correct work.

- **`monochange prepare` consumes changesets.** A completed release deletes `.changeset/*.md`, so their absence afterwards is not evidence that none were written. Grade the version outcome, and read intent from the transcript.
- **`preview` reports `version: null` for ungrouped packages.** Per-package versions live in `release_targets`. Asserting on the top-level `version` silently passes or fails for the wrong reason when no group exists.
- **Pre-1.0 bumps shift.** `0.5.2` plus a `minor` change plans `0.5.3`, because a major bump below `1.0.0` degrades to minor and minor to patch. Expectations against `0.x` fixtures must account for this.
- **`monochange step validate` passes with no config at all** and does not perform the release-time workspace load, so it cannot tell you a config is usable. Use `preview` (with a changeset present) for that.
- **`--jq` is a restricted subset.** Field access and `select` work; `length` and comma-separated filters silently print nothing. New structured assertions use Node to parse and check the complete JSON result; legacy scenarios still use Python.
