# Tooling and commands

## Environment

- Enter the reproducible development shell with `devenv shell` before using repo task commands.
- Install workspace tooling with `install:all` when needed.

Rust verification tasks (`test:cargo`, `test:cargo:expensive`, `test:docs`, `test:agent-evals`, `coverage:all`, `snapshot:check`, and `snapshot:update`) default `SSL_CERT_FILE` to the Nix CA certificate bundle. This avoids concurrent macOS trust-store discovery when GitHub test clients initialize TLS, including clients using mocked HTTP endpoints. Set a nonempty `SSL_CERT_FILE` before running a task to use your own certificate bundle; the tasks preserve that value. The default applies only within these tasks.

## Common commands

- `monochange --help`
- `monochange help subagents`
- `monochange subagents pi`
- `monochange mcp`
- `monochange step validate`
- `build:all`
- `lint:all`
- `lint:architecture`
- `lint:leptoskit`
- `test:all`
- `test:agent-evals`
- `test:node`
- `coverage:all`
- `coverage:patch`
- `build:book`

## Useful task-specific commands

### Documentation

- `docs:check`
- `docs:update`

### JSON schemas

- `devenv shell schema:update`: regenerate current schema aliases and fixtures.
- `devenv shell schema:check`: verify current generated assets.
- `devenv shell schema:release:update`: regenerate release aliases and fixtures using the planned schema version; this wrapper does not write versioned copies.
- `devenv shell schema:release:check`: verify release aliases and fixtures.
- `devenv shell cargo xtask schema release update --versioned`: explicitly regenerate the planned version's schema copies and artifact fixtures.
- `devenv shell cargo xtask schema release check --versioned`: verify those versioned assets too.

Schema assets are generated from Rust types and documentation; do not hand-edit them. The schema-assets tests require the current schema and the versioned copy named by `SCHEMA_VERSION` to agree apart from `$id`. When documentation changes an unreleased schema's description, refresh its versioned copy with the explicit `--versioned` command. Preserve already published versioned contracts and older schema assets; a description correction does not require a wire-contract version bump.

### Skill evaluations

- `devenv shell eval:skill --list`: list scenarios and available skill variants.
- `devenv shell eval:skill:contract`: run 22 deterministic contracts without model calls.
- `devenv shell eval:skill --scenario <id> --variant package`: run one agent task; repeat either selector for a cohort.
- `devenv shell eval:skill --scenario <id> --variant shipping-current --variant concise --variant package --repeats 3`: compare installed guidance with repeated runs.
- `devenv shell eval:skill --scenario <id> --variant package --regrade`: grade saved execution without another agent call; outcomes and provenance are required.
- `test:node`: includes evaluation harness unit tests alongside npm tooling tests.

Use `--model`/`MONOCHANGE_EVAL_MODEL`, `--agent-bin`/`MONOCHANGE_EVAL_AGENT_BIN`, and `--timeout` for runtime selection. `--skill-source cli` reads guidance from the evaluated binary and cannot compare installed variants. `MONOCHANGE_EVAL_CLI_PATH` can pin a preserved executable snapshot named `monochange`; there is no fallback to a released PATH binary. See [the harness README](../../evals/monochange-skill/README.md) for authentication, isolation, and reporting requirements.

### Snapshots

- `snapshot:review`
- `snapshot:update`

### Readability and complexity

- `lint:monostyle`: score the repository with monostyle and fail when either the readability or the complexity score drops below 80. CI runs it in the `lint` job.
- `fix:monostyle`: apply monostyle's autofixable whitespace fixes.
- `monostyle check <path> --units`: list the worst-scoring functions in a path as a worklist.
- `monostyle rules`: list every rule and its threshold.

The 80 threshold is a project-wide score, passed as `--fail-under 80`; a `fail-under` key in `monostyle.toml` would instead set a floor for every individual file. Scores come from `monostyle.toml` at the repository root; only set fields override monostyle's defaults, and generated or vendored trees are ignored through `[rules.ignore]`. `lint:monostyle` runs as part of `lint:all`, and `fix:monostyle` runs as part of `fix:all`. The fixes are whitespace-only and deliberately do not conflict with rustfmt, dprint, or oxfmt.

### leptoskit UI component lints

- `lint:leptoskit`: run the official [`leptoskit lint`](https://leptoskit.dev/docs/guide/23-linting.html) rules for every `leptoskit.toml` in the repository (`component_attribute` and `markup_style` with `markup = "view"` denied). The task reads the `leptoskit_cli` pin from `[workspace.metadata.bin]`, verifies every `leptoskit*` source in the app, native, and extension lockfiles against that Git revision, and reuses a cached `.bin` install only when cargo's `.crates.toml`/`.crates2.json` records the pinned Git source for the `leptoskit` binary. A same-version registry install or a different revision is replaced with a forced `cargo install` of the pinned revision. `cargo bin --install` alone can keep a stale same-version binary because cargo-run-bin's install path does not include the Git source. For a one-off install use `cargo install --git https://github.com/leptoskit/leptoskit --rev <rev> --locked leptoskit_cli`; a bare `cargo install leptoskit_cli` installs the stale crates.io release and is not used by the lint task. Every direct `leptoskit*` dependency in `app/Cargo.toml`, `app/native/Cargo.toml`, and `extension/Cargo.toml` is pinned to the same immutable full rev, so `cargo generate-lockfile` keeps the source the guard expects; upgrade by changing all manifest revs, the `[workspace.metadata.bin]` rev, and the three lockfiles together in one deliberate change. Keep each workspace's `wasm-bindgen` package family pinned to the exact version embedded in the CLI's `wasm-bindgen-cli-support` (`=0.2.129` today) with direct `wasm-bindgen = "=0.2.129"` dependencies in the wasm-producing crates (`app`, `extension`, and `app/native/monochange_native_app`); the CLI rejects wasm whose bindgen ABI does not match its embedded version.
- Local one-shot from a project directory: `leptoskit lint --deny-warnings --no-clippy` (workspace `lint:clippy` remains the clippy gate).
- Copy the `[lint]` / `[lint.rules]` block from `app/leptoskit.toml` into each new leptoskit surface (`extension/`, `app/native/`, …).

### leptoskit builds and the managed CLI

- `scripts/leptoskit-cli.sh` is the one resolver for the provenance-verified CLI: it reads the pin from `[workspace.metadata.bin]`, checks the lockfiles, reuses a proven cache, and force-installs the pinned revision otherwise. With no arguments it prints the binary path; with arguments it runs the binary. Use it for `build`, `migrate`, and `workspace` commands — never install or resolve the CLI by another path. `scripts/lint-leptoskit.sh` calls the same resolver, so the 12 resolver guard tests (registry-same-version rejection, wrong-rev rejection, lock drift, binary ownership, failed install never executing a stale CLI, nightly/stable toolchain keys) cover every consumer.
- `build:web`: build both web frontends with the pinned CLI into one assembled site at `app/target/site` (`build:web --release` for the size/benchmark gates). The website config lives at `app/leptoskit.toml`, the projects config at `app/projects/leptoskit.toml`; both truthfully name the same backend package (`app.name = "monochange_app"`) while selecting distinct browser packages via `build.client_package` — the website entry is `monochange_app`, the projects entry is the dedicated `monochange_projects_web` cdylib (the shared `monochange_projects` library is never a browser package, so its product route code cannot leak into the website bundle). `leptoskit.workspace.toml` at the repository root declares website (landing), projects (product), native, and extension; `leptoskit workspace check` validates configuration only and is never deployment evidence.
- The `[[plugins]]` tooling plugin (`scripts/leptoskit-plugin/`) runs inside `leptoskit build` on the documented JSON stdin/stdout process protocol, before release precompression: it compiles both frontends' Tailwind sources to the shared `pkg/site/monochange_app.css`, minifies the wasm-split loader through scripts/minify-split-loader.ts, and places each frontend's bundle under `pkg/<namespace>/`. It resolves as `leptoskit-plugin-monochange-web-assets` on PATH (the shim beside `web-assets-plugin.ts` locates the TypeScript plugin through symlinks; the devenv profile and Docker provide it), so the same declaration works from every project root. Missing tools fail the build loudly; `leptoskit doctor` checks the tools a build needs (node, esbuild, tailwindcss, binaryen/wasm-opt).
- `leptoskit generate deploy` refuses this workspace: `monochange_core`, `monochange_github`, and `monochange_hosting` are path dependencies outside the `app/` workspace, and the exporter requires all path deps inside it (moving or vendoring them is explicitly out of scope). Generated Dockerfiles also default to a crates.io CLI install with no Git-revision option. The hand-maintained Dockerfile documents this and builds/verifies the same Git-pinned CLI instead; a refused generation is not a generated deployment.

`lint:leptoskit` runs as part of `lint:all`, `lint:push`, and the CI lint job.

### Autofix

- `fix:all`
- `fix:clippy`
- `fix:format`
- `fix:monostyle`
