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
15. `mc init`: broad auto-discovery across all ecosystems can walk the tree many times.
16. `mc check --fix`: manifest rewrite paths should report per-file progress and avoid rereading unchanged files.
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
- [ ] Document current benchmark baselines in PR notes.

### Phase 2 — Progress reporter coverage

- [ ] Add discovery progress phases: config load, ecosystem scan start/finish, package counts.
- [ ] Add prepare-release phases: load changesets, compute graph, plan versions, render changelogs, update files, lockfiles.
- [ ] Add provider phases: prepare API client, lookup existing release/PR, create/update release/PR, labels/automerge.
- [ ] Add registry phases: check package, rate-limit planning, placeholder publish per package.
- [ ] Ensure `--progress-format json` emits machine-readable events for every phase.

### Phase 3 — Timeout and hang safety

- [x] Add default HTTP connect/request timeouts for shared provider clients.
- [x] Ensure GitLab/Gitea/Forgejo use shared timeout-enabled client builders.
- [x] Add elapsed-time heartbeat for external process steps that do not emit output.
- [ ] Add context-rich timeout errors including provider, URL class, and package/tag/PR being processed.

### Phase 4 — Fix discovered bottlenecks

- [x] Fix Python `.egg-info` traversal so package metadata directories are skipped by suffix.
- [ ] Replace repeated ecosystem `WalkDir` scans with a shared repository file index where benchmark data justifies it.
- [ ] Cache parsed manifests across discovery, validation, linting, and release planning where lifetimes align.
- [ ] Deduplicate lockfile refresh and lockfile parsing by lockfile path.
- [ ] Parallelize independent provider/registry checks with bounded concurrency and progress.

## PR execution checklist

- [ ] Keep changes small enough for one reviewable PR per phase.
- [ ] Add or update tests for every executable changed line.
- [ ] Keep patch coverage at 100%.
- [ ] Run `cargo test`, `cargo clippy --workspace -- -D warnings`, and targeted benchmarks locally.
- [ ] Open PR and monitor all CI checks until green.
