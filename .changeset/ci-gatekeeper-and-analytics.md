---
monochange: feat
monochange_config: feat
monochange_schema: feat
---

# Add a config-driven `monochange ci` gatekeeper with the full workspace graph

`monochange ci` gates CI jobs from declarative `[ci]` configuration in `monochange.toml` instead of hand-listing packages per job:

- `[ci.jobs.<id>]` declares each job's **root** packages (globs allowed) plus optional non-package `paths` and an `always` override. Globs expand against every workspace package and transitive dependencies are derived, so a new `leptoskit_plugin_foo` crate is covered by `leptoskit_plugin_*` with zero configuration edits.
- The dependency graph comes from `cargo metadata --format-version 1 --no-deps --offline`, which resolves offline (the gate never downloads the crates.io index) and includes unpublished members, crates nested at any depth, `foo.workspace = true` inheritance, renamed dependencies, and `[target.'cfg(..)'.dependencies]` sections; monochange's manifest discovery adds every other ecosystem. A root `Cargo.toml` that Cargo cannot parse is an error carrying cargo's own diagnostics, never a silent fallback to discovery.
- Decisions are safe by default: unknown must mean run. An unconfigured `--job` id runs with `no ci rule for <id>`, and a changed file that no package owns, no job claims, and `ignore_paths` ignores runs every job with a reason naming the file.
- `--on-error run-all` fails the gate open: GitHub skips every job that needs a failed gate, and a skipped required check counts as passing, so a broken gate would let any PR merge. With `run-all`, any error after argument parsing prints a `::error title=monochange ci decide::` annotation, writes `decisions` with every configured job (plus every `--job` id) set to `true`, `affected-packages=[]`, and `run-all=true` to the outputs, and exits 0; the default `--on-error fail` stays loud. A failure writing `$GITHUB_OUTPUT` fails the command instead of silently dropping the outputs.
- Providers (`opencode-go`, `openrouter`, `cloudflare`, `jev`, `openai-compatible`) are a preset table plus one OpenAI-compatible client making **one** request per `decide` run covering every job. With `ai_can_skip = false` (the default) a provider can only turn `n` into `y`; unconditional, `always`, and learned-rule verdicts are never overridden; provider failures fall back to the graph.
- `--format github-outputs` writes exactly `decisions` (compact JSON of job id to boolean), `affected-packages`, and `run-all`, so consumers gate jobs with `if: fromJSON(needs.<job>.outputs.decisions)['<id>']`.
- New `monochange ci check [--workflow <path>]...` fails with the full list of configuration problems: unknown root packages, zero-match package or path globs, jobs with no trigger, non-kebab-case ids, and workflow `decisions` references that point at unconfigured or never-referenced jobs.
- `monochange ci auth`, `verify`, `analytics`, and `history` keep their purpose, now keyed by configured job ids; `verify` still learns override rules from false negatives.
