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
node evals/monochange-skill/run.ts --list
node evals/monochange-skill/run.ts --all --variant package
node evals/monochange-skill/run.ts --scenario breaking-change-changeset --variant package
node evals/monochange-skill/run.ts --all --variant baseline --variant package --repeats 3
```

Useful flags: `--model` (or `MONOCHANGE_EVAL_MODEL`), `--timeout`, `--repeats`, `--instruction-variant`, `--skill-source`, `--list`, `--regrade`. `--instruction-variant` swaps in the wording from a scenario's `variants` map, which is how different phrasings of the same task are compared. `--all` is the default; passing `--scenario` narrows the selection.

`--skill-source installed` (the default) copies the variant into the run workdir as a project skill. `--skill-source cli` installs nothing: the agent starts with only the toolkit and has to find the guidance through the CLI, which is the path `monochange skill` serves. A scenario can pin the channel with a top-level `"skillSource": "cli"`. Runs are keyed by variant plus channel, so `package` and `package-cli` never overwrite each other.

### Prerequisites

1. Build the CLI the fixtures are graded with. The harness resolves `<repo>/target/debug/monochange` and never falls back to `PATH`, because a released `monochange` may have a different command surface and grading against it measures nothing:

   ```sh
   devenv shell -- cargo build -p monochange
   ```

2. The agent runtime must be authenticated. Runs use `--setting-sources project` and `--strict-mcp-config`, so the operator's personal hooks, plugins, and MCP servers stay out of the measurement.

## How isolation works

Each run copies a fixture into `.work/runs/<scenario>__<variant>__<n>` and installs the chosen skill as `<workdir>/.claude/skills/monochange`. Because the run uses `--setting-sources project`, that copy is the only `monochange` skill the agent sees — a globally installed skill of the same name cannot leak in.

The workspace `monochange` binary is prepended to the child's `PATH`, so `monochange ...` inside the fixture resolves to the build under test.

**Inherited repository policy reaches the agent.** Run workdirs live under this repository, so an agent working in one can read the monochange `AGENTS.md` that sits above `.work/`. That file forbids publishing packages and using registry credentials, which is the correct behavior to encode in a scenario rather than fight: a prompt that asks an agent to publish will get a refusal that is right about the policy even when it looks wrong about the scenario. Write scenarios that grade the gate — did the agent consult readiness and report what it found — rather than grading an action the repository forbids.

## Scenario inventory

29 scenarios, 162 checks: 12 agent scenarios and 17 agent-free contracts. `node evals/monochange-skill/run.ts --list` prints the current set.

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

Go and Dart have fixtures but no scenario yet. Deno has neither.

## Adding a skill variant

Copy a complete skill into `skill-variants/<name>/` (`SKILL.md` plus `skills/` and `examples/`) and pass `--variant <name>`. Comparing two variants is how a skill change is shown to fix a failure rather than merely changing the text. Keep one variant frozen so recorded comparisons stay meaningful; `skill-variants/freeze-baseline.sh` re-freezes `baseline` from a git ref.

The built-in `package` variant needs no copy: it installs `packages/monochange__skill`, the published artifact, so the thing measured is the thing that ships. A directory named `package` under `skill-variants/` would take precedence.

When both a frozen variant and `package` exist, the runner defaults to the frozen one. Pass `--variant package` explicitly to measure the live skill.

## Re-grading without re-running the agent

Agents are slow and stochastic, so a grader fix should not cost another round of runs:

```sh
node evals/monochange-skill/run.ts --all --variant package --regrade
```

This re-applies the current checks to the saved workdirs and transcripts. Use it after correcting a check, and re-run the agent only when the scenario itself changed.

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
- **`--jq` is a restricted subset.** Field access and `select` work; `length` and comma-separated filters silently print nothing. Use `python3` for structured assertions.
