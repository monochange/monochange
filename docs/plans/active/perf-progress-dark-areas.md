# Performance and progress dark-area hardening

## Goal

Make monochange feel alive and predictable in lesser-used ecosystems and providers by preventing commands from silently exceeding ~5 seconds without progress, and by adding benchmark coverage for discovery, provider/network, git-history, and external-command paths that are not yet battle-tested.

## Success criteria

- Commands that may take >1s emit human progress and JSON progress events.
- Provider API operations have explicit timeouts and meaningful status/error context.
- Lesser-used ecosystem discovery paths have scalable benchmarks at 50/100/500 packages.
- Mixed-ecosystem discovery is benchmarked to detect repeated full-tree walks.
- Any discovered >5s bottleneck gets either fixed or documented as a follow-up with a benchmark.
- CI remains green with 100% patch coverage.

## Top suspected bottlenecks

1. npm/pnpm/Bun discovery: workspace globs plus fallback full-tree `WalkDir` scans.
2. Deno discovery: glob expansion plus broad fallback manifest discovery.
3. Python discovery: glob expansion and broad `pyproject.toml` scanning.
4. Go discovery: full-tree `WalkDir` for `go.mod` in large repos.
5. Mixed ecosystem discovery: repeated independent full-tree walks per enabled ecosystem.
6. Versioned file validation: already fixed for inherited globs; keep benchmarks as guardrails.
7. Large lockfile updates: npm/pnpm/Bun/Cargo lockfiles may be parsed or rewritten per package instead of per lockfile.
8. External lockfile refresh commands: `npm`, `pnpm`, `bun`, `cargo`, `dart`, `go` commands can hang without heartbeat.
9. Provider release publishing: GitLab/Gitea/Forgejo paths are less used and depend on network/API behavior.
10. Provider release PR creation/update: existing-PR lookup, branch push, labels, and update calls can look hung.
11. Registry readiness checks: package availability checks may be sequential or blocked by registry slowness.
12. Placeholder publish: dry-run and real publish paths need visible package-by-package progress.
13. Git provenance/diagnostics: deep history and `git log --follow` style operations can become slow.
14. Release-record reconstruction: tag/commit discovery can be slow in repos with many tags.
15. `monochange init`: broad auto-discovery across all ecosystems can walk the tree many times.
16. `monochange check --fix`: manifest rewrite paths should report per-file progress and avoid rereading unchanged files.
17. Lint suites: each ecosystem linter may reread manifests already parsed during discovery.
18. MCP operations: agent callers need JSON progress or structured “still working” output for long tasks.
19. Provider HTTP clients: missing explicit connect/request timeouts can create indefinite waits.
20. Progress gaps: any command step with no output for >5s feels broken even when work is progressing.

## Phased plan

### Phase 1 — Benchmark guardrails and audit visibility

- [x] Add benchmarks for npm/pnpm/Bun discovery at 50/100/500 packages.
- [x] Add benchmarks for Deno discovery at 50/100/500 packages.
- [x] Add benchmarks for Python discovery at 50/100/500 packages.
- [x] Add benchmarks for Go discovery at 50/100/500 packages.
- [x] Add a mixed-ecosystem discovery benchmark with all lesser-used ecosystems enabled.
- [x] Document current benchmark baselines in PR notes.

Current generated discovery baseline from `cargo bench -p monochange --bench ecosystem_discovery -- --sample-size 10 --measurement-time 1` on 2026-06-01:

| Ecosystem    | 50 packages | 100 packages | 500 packages |
| ------------ | ----------: | -----------: | -----------: |
| npm/pnpm/Bun |     8.67 ms |     16.79 ms |     87.26 ms |
| Deno         |     6.21 ms |     12.33 ms |     64.88 ms |
| Python       |     6.84 ms |     13.59 ms |     70.06 ms |
| Go           |     5.66 ms |     11.53 ms |     61.15 ms |
| Mixed        |    46.17 ms |     92.76 ms |    532.50 ms |

Existing fixture discovery baselines: Dart 2 packages: 744 µs, Dart 11 packages: 3.40 ms, Dart 51 packages: 15.14 ms, Cargo workspace: 204.50 ms.

### Phase 2 — Progress reporter coverage

- [x] Add discovery progress phases: config load, ecosystem scan start/finish, package counts.
- [x] Add prepare-release phases: load changesets, compute graph, plan versions, render changelogs, update files, lockfiles.
- [x] Add provider phases: prepare API client, lookup existing release/PR, create/update release/PR, labels/automerge.
- [x] Add registry phases: check package, rate-limit planning, placeholder publish per package.
- [x] Ensure `--progress-format json` emits machine-readable events for every phase.

### Phase 3 — Timeout and hang safety

- [x] Add default HTTP connect/request timeouts for shared provider clients.
- [x] Ensure GitLab/Gitea/Forgejo use shared timeout-enabled client builders.
- [x] Add elapsed-time heartbeat for external process steps that do not emit output.
- [x] Add context-rich timeout errors including provider, URL class, and package/tag/PR being processed.

### Phase 4 — Fix discovered bottlenecks

- [x] Fix Python `.egg-info` traversal so package metadata directories are skipped by suffix.
- [x] Replace repeated ecosystem `WalkDir` scans with a shared repository file index where benchmark data justifies it. Decision: not justified in this PR; generated mixed discovery is about 0.53 s at 500 packages and still well below the 5 s feedback threshold, so keep the benchmark guardrail and defer shared indexing until a real repository exceeds the progress threshold.
- [x] Cache parsed manifests across discovery, validation, linting, and release planning where lifetimes align. Decision: not justified in this PR; current benchmark data points to sub-second discovery, and broader cache lifetimes need a dedicated design to avoid stale manifests during fix/apply flows.
- [x] Deduplicate lockfile refresh and lockfile parsing by lockfile path. Decision: covered by heartbeat progress for silent external lockfile commands; deeper lockfile deduplication remains a separate perf refactor because no benchmark in this PR shows a >5s lockfile parse bottleneck.
- [x] Parallelize independent provider/registry checks with bounded concurrency and progress. Decision: provider calls now have bounded HTTP timeouts and explicit progress phases; concurrency can be added later with provider-specific rate-limit semantics instead of changing execution ordering in this hardening PR.

## PR execution checklist

- [x] Keep changes small enough for one reviewable PR per phase.
- [x] Add or update tests for every executable changed line.
- [x] Keep patch coverage at 100%.
- [x] Run targeted `cargo test` commands and targeted benchmarks locally.
- [x] Open PR and monitor all CI checks until green.
