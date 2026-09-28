# Findings: evaluating the monochange skill against real release-planning tasks

This is the record of what the evaluation established, including the parts that did not confirm the initial hypothesis. It is written so a later reader can tell which claims are measured and which are not.

## Question

monochange spans six package ecosystems, a configuration language, a changeset format, and a publish lifecycle. The skill has to carry all of that. The question was whether an agent holding the skill can complete realistic release-planning work in a real monorepo — adopting monochange, authoring changesets that match the actual change, configuring versioned files for ecosystems that need it, and stopping short of publishing — and where the skill leaves it guessing.

A second question ran alongside: whether the guidance is reachable at all. The skill previously reached an agent only by downloading it through the `skills` npm CLI. It now ships inside the binary, which is the path this suite exercises.

## Method

Eight scenarios, each a real task in a throwaway monorepo, graded by the real `monochange` binary plus targeted file and transcript assertions. Fixtures span the ecosystems monochange supports:

| Fixture                        | Shape                                                                          |
| ------------------------------ | ------------------------------------------------------------------------------ |
| `mixed-monorepo`               | Cargo crates plus scoped npm packages, no config                               |
| `npm-monorepo`                 | pnpm workspace, three packages in one version group, a `user` changelog stream |
| `rust-workspace`               | two independently-versioned crates, one depending on the other                 |
| `python-uv`                    | uv workspace, two `pyproject.toml` packages, no config                         |
| `go-modules`, `dart-workspace` | Go modules and a pub workspace, used by the discovery probes                   |

One scenario (`cli-self-sufficiency`) runs no agent and pins the CLI contract deterministically, because a model that already knows the answer will not go looking and grading its transcript would fail correct work.

Runs use `--setting-sources project` and `--strict-mcp-config` so the operator's personal hooks, plugins, and MCP servers stay out of the measurement. The workspace binary is resolved from `target/debug/monochange` and injected through the child environment; there is no fallback to a released binary on `PATH`, because grading against a different version measures the machine rather than the skill.

## Result

The suite grew to 28 scenarios and 158 checks covering all six ecosystems, version computation, changesets, versioned files, lockfiles, the release lifecycle, publishing gates, changelog outputs, linting, CLI surface contracts, safety, and provider configuration. Eleven scenarios drive an agent; seventeen are agent-free contracts that run in CI at no cost.

Every check passes against the shipped skill once the defects below were fixed: 28 runs, no failing checks, 143k tokens, about $10 of agent time. One model, one run per cell, so the pass rate is a regression baseline rather than a precision measurement.

Each CLI defect arrived with a failing test written first and a passing test after. The Rust workspace now carries 3707 passing tests, and the fixes are covered at three levels: unit tests in the owning crate, integration tests in `crates/monochange_integration_tests` with file fixtures and `insta` snapshots, and the eval scenarios here.

The more important result is what it cost to get there: **nine real defects in the CLI and five in the skill**, several of which made advertised workflows impossible to complete or silently produced wrong release plans. Each is listed below with the experiment that proved it.

Round two added the wider matrix. Its value is not that more scenarios pass — it is that six of the nine CLI defects were found by checks written for surfaces the first eight scenarios never touched. A release-planning suite that only exercises adoption and changeset authoring leaves most of the tool unmeasured.

The honest caveat, carried over from the Pina suite this harness is ported from: the pre-fix skill also passed most scenarios, because a strong model recovers from stale and missing guidance by reading the error messages. No claim is made here that any single skill edit turned a failing task into a passing one. The suite's value is as a regression gate over the surfaces most likely to break.

## Defects found in round two

These are the defects the widened matrix caught. Each was reproduced against the built binary before being fixed, and each now has a failing-then-passing test or a scenario check.

1. **A Go dependent's `require` directive was never rewritten.** A Go service that depended on a workspace library kept `require github.com/acme/core v1.2.0` after the library moved to 1.3.0, so the released service shipped pinned to a version that no longer existed. Go's internal-dependency rewrite was dead for the same reason as defect 2 below; fixing the matcher fixed both.

2. **`versions sync` never matched a nested Go module path.** `detect_go_changes` compared the full module path from a `require` directive (`github.com/acme/core`) against a set of derived short names (`core`), so the membership test could never succeed. Detection and application also disagreed about which key identified a dependency.

3. **The first Go fix over-matched and corrupted third-party pins.** Fixing 2 by matching a require's last path segment meant `github.com/other/core` — an unrelated module — was rewritten to the workspace's version. Measured directly: the keys reaching the writer were `["github.com/acme/core", "github.com/other/core"]`, so the detect step was inventing the second change. The correct rule resolves a require against the module path each workspace package declares in its own `go.mod`, and leaves anything it cannot resolve exactly alone.

4. **`monochange create --dry-run` wrote the changeset.** The `--dry-run` flag was parsed and then dropped on the floor for this one step: the file landed in `.changeset/` and stdout said "wrote change file". The skill teaches agents to dry-run before mutating, and `create` writes release intent, so a dry run silently changed the next person's release. The step now prints `would write change file` with the rendered content and writes nothing.

5. **Quoted YAML keys were never matched in `pnpm-lock.yaml`.** Real pnpm lockfiles quote scoped package names (`'@acme/api':`), and `parse_yaml_line` used the raw left-hand text as the lookup key, so the entry was missed. The lockfile appeared in `changed_files` while its bytes were written back unchanged — worse than not listing it, because the release plan claimed a refresh that never happened. Measured: the same workspace with an unquoted key updated correctly and with a quoted key did not.

6. **The same quoting defect existed in Dart**, twice: a quoted dependency key in `pubspec.yaml` was never updated, and the dependency-sorted lint compared source key order against unquoted parsed keys, so a correctly sorted section with quoted keys was reported unsorted forever.

7. **`preview` could return a stale cached plan.** The prepared-release cache was keyed on git state, so editing a changeset's severity in place — same path, same filename — returned the previous plan. Measured: writing `minor`, then rewriting the same file to `major`, still reported the minor version; clearing `.monochange/local` made it correct. `preview` is the surface a maintainer reviews before a release, so this could ship a severity nobody chose.

8. **`monochange init` emitted invalid TOML for scoped package ids** (round one). `@acme/sdk` produced `[package.@acme/sdk]`, which is not a legal TOML key, so the config the tool had just written failed to parse.

9. **Python packages could not be released at all** (round one). `PythonAdapter::load_configured` returned `Ok(None)` unconditionally, so a configured Python package was never loaded by the release-time workspace loader: the generated config passed `step validate` and then failed the first real release command.

## Defects the evaluation found in the skill

1. **The skill said `monochange step validate` rejects cross-stream changesets. It does not.** Measured directly on a file mixing a default-stream type with a user-stream type:

   | Command                    | Result                                                            |
   | -------------------------- | ----------------------------------------------------------------- |
   | `monochange step validate` | exit 0, "workspace validation passed"                             |
   | `monochange check`         | exit 0, "no issues found"                                         |
   | `monochange preview`       | exit 1, "changeset targets resolve to multiple changelog streams" |

   This is the most damaging of the five, because it is a false negative in the agent's own verification loop: author one file covering both audiences, run the command the skill names, watch it pass, and report success on a release plan that cannot prepare. The skill now names the preview as the command that catches it and states explicitly that `validate` and `check` do not.

2. **Python version writing was undocumented.** A Python package's own `[project].version` is not rewritten by a release unless the config declares an explicit entry whose `fields` include `version`:

   ```toml
   [[package.acme-insight.versioned_files]]
   path = "packages/insight/pyproject.toml"
   type = "python"
   fields = ["version"]
   ```

   Verified by experiment: without the entry a real `prepare` plans the new version, rewrites internal dependency constraints, and leaves `[project].version` stale; a bare-string `versioned_files` entry also leaves it stale, because the version is only written when `fields` names it. Python has no built-in manifest version writer — the pipeline covers only cargo, npm, deno, and dart.

3. **The command inventory was a generation behind the CLI.** The skill taught `monochange step create-change-file`, `step prepare-release`, `step discover`, and `step diagnose-changesets` as the way to do those things, while the CLI had grown first-class `create`, `prepare`, `preview`, `discover`, `config`, `affected`, `diagnose`, `next`, and `versions list|sync`. An agent following the skill would use workable but stale spellings and never learn the shorter surface.

4. **`monochange versions` was documented inconsistently.** One line described `versions sync --strategy`, another described bare `monochange versions` with the same flags, and neither mentioned that the bare form is deprecated in favor of `versions sync`, nor that `versions list` exists for a read-only inventory.

5. **A stale field name.** The skill told agents to verify `compatibilityEvidence` in the preview; nothing emits that. The classification report field is `decision.compatibility_impact` and the preview plan field is `compatibility_evidence`.

## What the eval infrastructure pinned down

Behaviours that are easy to assume wrong, established by running the binary rather than reading the docs. These now live in scenario checks so a regression fails loudly:

- **`preview` reports `version: null` when packages are not version-grouped.** Per-package versions live in `release_targets`. A group releases through the top-level `version`/`group_version` fields. Checks that assert on `version` alone silently pass for ungrouped repos.
- **`prepare` consumes and deletes changesets.** A completed release leaves no `.changeset/*.md` behind, so their absence is not evidence that none were written.
- **Pre-1.0 versions shift the bump.** `0.5.2` plus a `minor` change plans `0.5.3`, not `0.6.0`, because a major bump below `1.0.0` degrades to minor and minor to patch. Any expectation written against a `0.x` fixture has to account for this.
- **`step validate` passes in a directory with no config at all**, reporting "workspace validation passed". It is a parse-and-target check, not a presence check.
- **Dart workspace roots are discovered as releasable packages** with a null version, so `init` puts the root `pubspec.yaml` into `[package.*]` and the default group. A Dart fixture does not have the package count its directory layout suggests.
- **Go versions come from git tags, not manifests.** A Go package needs `tag = true` plus `initial_version` before it plans anything. With both modules tagged at their initial versions and a `minor` changeset on `core`, the plan moves `core` to `1.3.0` and propagates `service` to `1.0.1` — but `changed_files` is empty: the `require github.com/acme/core v1.2.0` line in `service/go.mod` is not rewritten, so the service would ship pinned to a core version that no longer exists. The rewrite matches on the module path against workspace package names, and a nested module path (`github.com/acme/core` against a package named `core`) does not match. Reproduced but not fixed, and recorded here as an open question rather than a resolved defect.
- **`monochange affected --changed-paths` must include the changeset path.** `affected` derives `changeset_paths` by filtering the changed-path list, so a changeset only counts toward coverage when it is part of the change set being evaluated. A check that passes only the source path sees an uncovered change even though release intent exists. This is how the policy behaves in CI, where the diff carries both.
- **`monochange notes --file` redirects rather than tees.** The note goes to the named path and stdout stays empty, so a CI consumer cannot double-print the artifact.
- **`--quiet` suppresses explicit JSON output entirely.** `monochange preview --format json --quiet` prints nothing, so a script must not combine them; the harness reads JSON by piping the normal run to a parser.
- **Configuring `lockfile_commands` replaces the built-in lockfile rewrite** rather than adding to it, verified by asserting the direct writer did not also fire. The command needs `shell = true` to use a redirection; without it the command is exec'd directly and `>>` is an argument.
- **`monochange init` writes `.github/workflows` only for the `github` provider**, and the generated config carries an active `[source]` table and no `[cli.*]` tables at all.
- **`monochange subagents --dry-run` writes nothing**, and the generated guidance names `monochange skill read monochange` first — pinned because the previous text taught a report field that does not exist.
- **A group's version target is reported by its own id.** Asserting on member ids fails when an agent names its group something other than `main`; grading the group's version and its member list is both stricter and name-independent.
- **Below 1.0.0 a `minor` change plans a patch and a `major` change plans a minor.** Two scenarios depend on this, so it is pinned directly rather than relied on incidentally.

## What the evaluation said about the agent, not the skill

The most encouraging result is a scenario that failed. The release scenario originally asked for a minor release and described two changes, one of which widened a tokens object that a `keyof typeof` union was derived from. The agent implemented the change, ran `monochange change classify`, and stopped:

> `@acme/tokens` is flagged **breaking** … `SpacingToken = keyof typeof spacing` widens from 5 to 7 members, and TypeScript's assignability check isn't symmetric … a major changeset on `@acme/tokens` forces the **entire group** to the next major version, not `2.4.0`. Before I write changesets and run `prepare`, how do you want to handle this?

It offered three options — accept the major, override with justification, or rework the token export — and asked rather than guessing. That is the behaviour the skill asks for: the analysis was high-confidence, the requested version contradicted it, and the group coupling meant the conflict could not be absorbed silently.

The scenario was wrong, not the agent. Two lessons came out of it:

- **A scenario's expected outcome must not contradict the tool's own evidence.** The premise "widening a token scale is a minor change" is false for a type derived with `keyof typeof`, and the analyzer was right to say so. Scenarios that assert a version should use changes whose severity is not genuinely contested, or state the intended severity explicitly and test something else.
- **The semantic analyzer's verdict depends on the environment.** In a run where TypeScript could not be resolved, the same change classified as `impact: unmodeled`, `bump: patch`, `review_required: true`; in the agent's workdir, with the workspace compiler available, it was `additive`, `minor`, `complete`. A scenario that hinges on a modeled verdict is only reproducible when the fixture's toolchain is installed. The suite now uses semantic classification as context an agent may consult, never as the premise of a check.

The dominant risk was not the agent, it was the grader. Both round-one failures were defects in the evaluation, not in the agent's work:

- **A changeset file's absence is not a failure.** The Python scenario asserted a `.changeset/*.md` file still existed at grading time. The agent had completed the task correctly — it adopted monochange, added typed `versioned_files` for both packages, wrote both versions, and moved the internal dependency constraint — but running `prepare` consumed the changeset. The check now grades the outcome and reads intent from the transcript.
- **An ambiguous prompt gets the safest reading.** "Do the release preparation for it, but do not publish — I want to review the diff first" was read as _preview only_, which is a defensible interpretation of "review the diff". The prompt now says explicitly to write the bumps and changelog entries into the tree. A scenario that grades mutation has to ask for mutation unambiguously.

Both were fixed with `--regrade`, which re-applies current checks to saved workdirs and transcripts, so a grader fix costs no new agent runs. That flag is the single most valuable part of the harness.

The three failure modes to keep avoiding, carried over from the Pina suite and confirmed here:

- **Matching one spelling.** Grade the property, not the phrasing. The breaking-change scenario grades the resulting version rather than the string `major`, so a configured type that maps to a major release passes too.
- **Matching the transcript for a file property.** Assert the artifact, not the sentence describing it.
- **Matching content the agent read.** Transcript checks see only assistant-authored text and tool inputs, or the skill's own documentation would satisfy them.

Prefer a `command` check whenever the CLI can answer the question, and prefer an outcome over a spelling.

## Grader lessons

The dominant risk is not the agent, it is the grader. Round two produced five grader defects against zero agent failures, and every one was a false negative on correct work:

- **A mention is not an action.** A safety check forbade `npm publish` in the transcript. The agent correctly wrote "npm publish is still blocked" in its answer and executed nothing — and failed the check for saying the words. Transcript checks see assistant-authored prose, so a check forbidding a _command name_ fires on an agent that explains the command is blocked. Grade the workspace for actions and the transcript for decisions.
- **A completed mutation is not a missing artifact.** A scenario asserted a changeset file still existed at grading time; the agent had already run `prepare`, which consumes it. Grade the outcome and read intent from the transcript.
- **An earlier check can consume what a later one needs.** A Go scenario's second `prepare` had no changesets left, so its changed-file assertion failed on a correct workspace. Any check that re-runs a consuming command must restore its input first.
- **Grade invariants, not literals.** A lockfile check asserted `version: 2.4.0`; an earlier check had already advanced the version, so the literal was wrong while the behavior was right. It now asserts that the lockfile carries whatever version the manifest declares.
- **A group's id is the agent's choice.** A Dart check keyed on a group named `main`; the agent named its group `acme` and was failed for it. Grade a group's version and membership, not its name.

## Limits

- **One model, one run per cell.** Single-run pass rates in this suite are noise. A scenario that flips between runs is showing model variance until repeats say otherwise.
- **The fixtures are not production repositories.** They are small workspaces with no lockfiles, no CI, and no published history. Real registry publishing, release records read back from git history over a long history, and provider flows are not exercised; the scenarios that touch publishing grade the gate and the refusal, not the upload.
- **Semantic classification verdicts are not graded.** FINDINGS established earlier that an analyzer verdict depends on the fixture's installed toolchain — the same diff classified as `minor` with the workspace compiler present and `patch` without it — so a check premised on a verdict would grade the machine. Scenarios may consult classification, but none depends on its answer.
- **Lockfiles and the release lifecycle are covered by contract, not by an agent.** The lockfile rewrite, the lifecycle gates, and the notes selection are deterministic properties, so they are pinned with `command` checks rather than a stochastic run. That is the right trade for cost, and it does mean no agent scenario exercises those paths end to end.
- **The cost of a full pass is real.** Twenty-eight scenarios took roughly twenty to thirty minutes and five to ten dollars of agent time. Use `--scenario` for iteration and `--regrade` after a grader fix, which re-applies checks to saved workdirs without paying for another run.
