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

### Autofix

- `fix:all`
- `fix:clippy`
- `fix:format`
