# Design: monochange skill evaluations

Working design notes for the eval suite. The reference implementation is the pina harness at `pina-rs/pina/worktrees/skill-migration-evals/evals/pina-skill`; this suite follows its architecture and reuses its lessons.

## Goal

Measure whether an agent holding the monochange skill can complete real release planning work in a real multi-ecosystem monorepo: adopt monochange, author changesets that match the actual change, configure versioned files and groups correctly, preview versions, and gate publishing. Grade the artifacts the agent leaves behind with the real `monochange` binary.

## Why artifact-based grading

An agent that explains release planning beautifully but writes a changeset with the wrong target or bump has failed. Every scenario therefore runs a real change in a throwaway monorepo and grades with the same tools CI would: `monochange step validate`, `monochange check`, `monochange preview --format json`, `monochange discover --format json`, plus targeted file and transcript assertions.

## Harness layout

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
├── skill-variants/        complete monochange skills to compare
├── results/               transcripts and reports (git-ignored)
└── FINDINGS.md            what the evaluations established
```

## Scenario schema

Identical to the pina harness (`lib/types.ts`). Fields that matter:

- `id` — stable identifier, matches the file name.
- `title` — human label for the report.
- `fixture` — directory name under `fixtures/`.
- `prompt` — handed to the agent verbatim. Written as a user would: state the goal and the constraint, not the mechanism.
- `expectation` — what a correct solution looks like, quoted in the report.
- `skillSource` — optional per-scenario pin of `installed` (default) or `cli`.
- `agent` — set `false` for deterministic, agent-free contract tests.
- `checks` — graded assertions.

### Check kinds

| Kind         | Grades                                                          |
| ------------ | --------------------------------------------------------------- |
| `command`    | a shell command's exit status and output substrings             |
| `file`       | a file exists, and its contents match (or `absent` them)        |
| `absent`     | a path does not exist                                           |
| `transcript` | the agent's own text and tool inputs match (or avoid) a pattern |

Transcript checks look only at content the assistant authored. Tool results and skill files the agent read are excluded, so a check cannot accidentally match the skill's own documentation instead of the agent's work.

## Fixtures

Real monorepos, small enough to reason about, spanning the ecosystems monochange supports. Each fixture is a working repository the agent can run commands in.

| Fixture           | Shape                                                              |
| ----------------- | ------------------------------------------------------------------ |
| `empty-workspace` | git repo, no manifests, for the `init` scenario                    |
| `rust-workspace`  | Cargo workspace, two crates, one depending on the other            |
| `npm-monorepo`    | pnpm workspace, three TypeScript packages with internal deps       |
| `mixed-monorepo`  | Cargo crates + npm packages in one workspace, no `monochange.toml` |
| `python-uv`       | uv workspace, two `pyproject.toml` packages                        |
| `dart-workspace`  | pub workspace with two packages                                    |
| `go-modules`      | two Go modules in one repository                                   |

The `mixed-monorepo` and `python-uv` fixtures ship **without** a `monochange.toml`, because writing that config is the task.

## Grading rules learned from the pina suite

A wrong check is worse than no check, because it looks like evidence. Three failure modes produced false negatives there and apply here:

- **Matching one spelling.** A correct solution may write `fix` instead of `patch`, or quote an id that does not need quoting. Grade the property, not the phrasing.
- **Matching the transcript for a file property.** Assert the artifact.
- **Matching content the agent read.** Only assistant-authored text and tool inputs are visible to transcript checks.

Prefer a `command` check whenever the CLI can answer the question. Prefer an outcome over a spelling.

## Isolation

Each run copies a fixture into `.work/runs/<scenario>__<variant>__<n>` and installs the chosen skill as `<workdir>/.claude/skills/monochange`. Runs use `--setting-sources project` so a globally installed skill of the same name cannot leak in. The workspace `monochange` binary is resolved from `<repo>/target/debug/monochange` and prepended to `PATH`; the harness never falls back to a released binary on `PATH`, because grading against a different version measures nothing.
