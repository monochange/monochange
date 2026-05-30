# CLI startup and config-loading performance

## Problem statement

In large workspaces such as `/Users/ifiokjr/Developer/projects/solana_kit`, `mc` and `monochange` startup can take tens of seconds even for commands such as `--version`, `--help`, and no-argument invocation. Diagnostics showed the main cost is `monochange_config::load_workspace_configuration`, especially repeated expansion of inherited ecosystem-level `versioned_files` globs such as `**/pubspec.yaml`.

The current startup path also performs more work than required:

- version output should not load configuration at all
- some help paths need only configured CLI command metadata, not full package and versioned-file validation
- inherited ecosystem globs are validated repeatedly per package
- root help currently loads configuration twice in some paths

## Goals

- Make `mc --version`, `mc -V`, `monochange --version`, and `monochange -V` config-free and millisecond-fast.
- Make help and command-dispatch paths perform the minimum configuration work needed for the requested operation.
- Make full configuration loading fast for inherited ecosystem glob scenarios, with no repeated filesystem walks for the same glob.
- Add regression benchmarks for inherited ecosystem `versioned_files` globs across all supported ecosystems.
- Add CLI startup benchmarks that make slow paths visible for `mc` and `monochange` commands.
- Keep behavior compatible for validated commands that intentionally need full package and versioned-file checks.

## Non-goals

- Do not change release semantics or versioned-file update behavior.
- Do not remove validation from commands whose purpose is validation.
- Do not publish packages, modify tags, or run release workflows.
- Do not keep temporary diagnostic `eprintln!` instrumentation in the final branch.

## Root cause notes

Diagnostic build findings in `solana_kit`:

- `load_raw_configuration` and TOML parsing are fast, usually a few milliseconds.
- `[ecosystems.dart].versioned_files = [{ path = "**/pubspec.yaml", ... }]` is inherited by every Dart package.
- The same glob expansion ran once for the ecosystem and again for each package.
- Each expansion took roughly 0.5-0.7s in the real repo.
- `validate_package_and_group_definitions` accounted for roughly 25-28s per full config load.
- `mc --help` loaded config twice in the instrumented path, making it roughly twice as slow.
- `monochange --version` lacks the sync version fast path that `mc` has.

## Affected files and crates

Likely files:

- `crates/monochange/src/bin/mc.rs`
- `crates/monochange/src/main.rs`
- `crates/monochange/src/lib.rs`
- `crates/monochange/src/cli.rs`
- `crates/monochange/src/cli_runtime.rs`
- `crates/monochange_config/src/lib.rs`
- `crates/monochange_config/src/__tests__/lib_tests.rs` or adjacent module tests
- `crates/monochange/benches/cli_commands.rs`
- new or existing config-loading benchmark file under `crates/monochange_config/benches/` or `crates/monochange/benches/`
- `.changeset/*.md`

## Design direction

### 1. Config-free version handling

- Keep `mc` sync version handling.
- Add equivalent sync version handling to the `monochange` binary.
- Ensure version fast paths tolerate global flags only when they can do so without config, or intentionally fall back for unsupported combinations.
- Verify no code path used for version calls `build_command_for_root`, `cli_commands_for_root`, or `load_workspace_configuration`.

### 2. True base command construction

Current `build_command_for_root` loads configuration through `cli_commands_for_root`. For pre-parse/fast-path checks, introduce a config-free command builder that uses only static/default commands.

Possible shape:

```rust
fn build_base_command(bin_name: &'static str) -> clap::Command
```

Use it for:

- detecting `DisplayVersion`
- detecting built-in root help when custom commands are not required
- parsing clearly built-in commands that can dispatch without custom CLI metadata

### 3. Split configuration loading by purpose

Introduce an internal load mode or loader API so callers can request only the work they need.

Potential modes:

```rust
enum ConfigurationLoadMode {
	CliOnly,
	WorkspaceMetadata,
	FullyValidated,
}
```

Expected behavior:

- `CliOnly`: parse raw config and normalize `[cli.*]` enough for help/command dispatch; skip package path checks, manifest checks, glob expansion, versioned-file content validation, and heavyweight workspace validation.
- `WorkspaceMetadata`: parse package/group definitions enough for choices and command defaults, but avoid expensive validation where possible.
- `FullyValidated`: current validated behavior, optimized with caching/deduplication.

If a smaller refactor is safer, start with dedicated functions:

- `load_cli_configuration(root)`
- `load_workspace_configuration(root)`
- shared lower-level helpers for raw parsing and normalization

### 4. Deduplicate inherited ecosystem versioned-file validation

The final fully validated path should not repeatedly expand the same glob inherited from ecosystem settings.

Approach options:

- Carry provenance on `VersionedFileDefinition` so inherited definitions can be skipped during per-package validation after ecosystem-level validation.
- Or use a validation cache keyed by `(path, ecosystem_type)` while preserving owner-specific diagnostics.
- Prefer both if feasible: avoid scheduling duplicate work, and have a cache as a safety net.

Minimum cache key:

```rust
(root, versioned_file.path, versioned_file.ecosystem_type, versioned_file.regex, versioned_file.fields)
```

For glob support-type validation, path and ecosystem type are the key parts. Include enough fields to avoid accidentally reusing results for incompatible validation behavior.

### 5. Lazy command execution work

Audit command dispatch so each command pays only for the configuration shape it needs:

- version: no config
- built-in command help: no config unless listing custom commands
- root help / `help`: CLI-only config
- custom command help: CLI-only config
- `lint --list` / `lint --explain`: no workspace validation unless rule behavior requires config
- `mcp`: avoid workspace validation at server startup; validate inside tools that need it
- `step:config`: parse/normalize config but avoid package/glob validation unless explicitly requested
- `step:validate`, `check`, release planning, publish/readiness commands: full validated config, but optimized
- custom command execution: load CLI metadata first; load full workspace only when a step actually needs it

## Benchmark plan

### Specific inherited ecosystem glob benchmark

Create fixtures that model this shape:

```toml
[ecosystems.<ecosystem>]
versioned_files = [
  { path = "**/<manifest>", type = "<ecosystem>" }
]

[package.pkg_000]
path = "packages/pkg_000"
# inherits ecosystem versioned_files
```

Add one benchmark case per ecosystem:

- Cargo: `**/Cargo.toml`
- npm: `**/package.json`
- Deno: `**/deno.json`
- Dart: `**/pubspec.yaml`
- Python: `**/pyproject.toml`
- Go: `**/go.mod`

Each benchmark should create enough packages and extra matching files to reproduce repeated glob cost. Suggested shape:

- 50 packages
- one manifest per package
- nested/generated directories with additional matching manifests where useful
- same ecosystem-level glob inherited by all packages

Measure:

- full `load_workspace_configuration`
- CLI-only config load once introduced
- optionally direct versioned-file validation cache behavior

Acceptance target:

- full config load for this scenario should be under 1s locally, preferably much lower
- repeated inherited glob validation should execute one filesystem walk per unique glob/ecosystem, not one per package

### CLI command startup benchmarks

Extend existing CLI command benchmarks to cover both binaries and common command classes:

- `mc --version`
- `mc -V`
- `monochange --version`
- `monochange -V`
- `mc --help`
- `monochange --help`
- `mc help`
- `mc help <custom-command>` with a fixture config
- `mc lint --list`
- `mc lint --explain <rule>`
- `mc step:config --format json`
- representative no-arg invocation/error path

Benchmarks should run against a fixture containing inherited ecosystem globs to prove config-free and CLI-only paths do not accidentally trigger full validation.

## Test plan

Add regression tests for:

- `monochange --version` does not load config, even when cwd has expensive config.
- `mc --version` still does not load config.
- root help loads configuration at most once.
- root help can render custom commands using CLI-only config without validating package paths/globs.
- inherited ecosystem versioned-file globs are validated once, not once per package.
- full validation still reports unsupported glob matches and missing files correctly.
- explicit validation commands still perform full validation.

Use test fixtures and counters where practical instead of relying on wall-clock timing in unit tests.

## Implementation checklist

- [ ] Remove temporary diagnostic instrumentation from `crates/monochange_config/src/lib.rs`.
- [ ] Add plan changeset.
- [ ] Add/repair sync version fast path for `monochange` binary.
- [ ] Introduce a truly config-free base command builder.
- [ ] Update `run_with_args_in_dir` to use config-free parsing for version and built-in help probes.
- [ ] Add CLI-only config loading path or equivalent focused parser.
- [ ] Route help/dispatch paths to CLI-only loading where possible.
- [ ] Add inherited glob validation cache/deduplication.
- [ ] Add tests for config-free and CLI-only behavior.
- [ ] Add inherited ecosystem glob benchmarks for Cargo, npm, Deno, Dart, Python, and Go.
- [ ] Extend CLI startup benchmarks for both `mc` and `monochange`.
- [ ] Run focused unit tests.
- [ ] Run benchmark script locally against the new fixture.
- [ ] Run `devenv shell lint:monochange` or the repo-required validation subset.
- [ ] Run `devenv shell coverage:all` and `devenv shell coverage:patch` before PR.
- [ ] Update this plan with decisions and final benchmark results.

## Acceptance checks

- `mc --version` and `monochange --version` are milliseconds in `solana_kit`.
- `mc --help` and `monochange --help` no longer perform full package/glob validation.
- Full config loading for inherited ecosystem glob fixtures is under 1s locally.
- Benchmarks cover all supported ecosystems with ecosystem-level glob inheritance.
- Tests prove expensive validation is not triggered for commands that do not need it.
- Patch coverage remains 100%.

## Open decisions

- Whether `step:config` should print fully validated config or a normalized-but-not-fully-validated config by default.
- Whether CLI-only parsing should live in `monochange_config` or in `monochange` as a narrower raw config reader.
- Whether glob cache should be a local validation context only or a more reusable config-loading primitive.

## Notes while implementing

- Do not optimize only `--version`; the real issue is over-eager config loading and repeated glob validation.
- Keep user-visible diagnostics accurate when validation is explicitly requested.
- Avoid hidden global caches unless they are scoped to one config load; stale filesystem results would be surprising.
- Prefer deterministic tests with injected counters/fixtures over time-based assertions.

## Implementation update (2026-05-30)

Implemented in this branch:

- Added `monochange_config::load_cli_commands(root)` to parse raw config and merge CLI command metadata without package/group/ecosystem validation.
- Changed CLI startup/help paths to use CLI-only loading for root help, no-arg help, `help`, `help <command>`, and `<command> --help`.
- Added a config-free `monochange --version` sync fast path and changed the `mc` sync version fast path to print to stdout.
- Added per-config-load glob validation dedupe for package/group `versioned_files`, so inherited ecosystem globs are not re-expanded once per package.
- Added regression tests for CLI-only config loading and version/root-help behavior with invalid package/versioned-file config.
- Added `cli_startup_help` Criterion cases for `mc --version`, root help, and command help.

Real `solana_kit` release-binary timings after warmup:

- `mc --version`: ~0.006s median
- `monochange --version`: ~0.005s median
- `mc --help`: ~0.006s median
- `mc` with no args: ~0.006s median
- `mc help release`: ~0.006s median
- `mc release --help`: ~0.007s median
- `mc step:config`: ~2.5s median, down from the previous repeated-glob 25s+ config-load behavior

Remaining possible follow-ups:

- Extend inherited-glob Criterion fixtures across Cargo, npm, Deno, Dart, Python, and Go.
- Split `step:config` onto a lighter config path if it should avoid full package/glob validation.
- Continue moving command-specific execution paths from full validated config to targeted loaders where safe.
