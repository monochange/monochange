# AGENTS

`monochange` is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems.

## Workspace essentials

- Tooling: use `cargo` in the `devenv` shell; repository task commands are exposed as `devenv` scripts. Enter the `devenv shell` before running repo commands (e.g. `devenv shell fix:all`). See https://devenv.sh/blog/2026/05/07/devenv-21-nix-with-zsh-fish-and-nushell-via-libghostty/#__tabbed_1_4
- Fix all warnings: `fix:all`
- Build: `build:all`
- Quality/typecheck: `lint:all`
- Validation: `monochange step validate`
- Patch coverage: every pull request must keep patch coverage at 100% for executable changed lines. Add or adjust tests until `coverage:patch` is green.
- Rust unit tests inside crate `src/` trees must live in a `__tests__/` directory next to the module under test, with file names matching `__tests__/<module_name>_tests.rs` (`lib.rs` → `__tests__/lib_tests.rs`, `mod.rs` → `__tests__/mod_tests.rs`). Reference them with `#[cfg(test)] #[path = "__tests__/<module_name>_tests.rs"] mod tests;` instead of inline test modules or sibling `__tests.rs` files. This rule does not apply to integration tests in `<crate>/tests/`; leave those files in the standard integration-test layout.
- `#[cfg(test)]` may only reference a test module and may appear at most once per source file. Keep test-only helpers, imports, state, and re-exports in test files; if helpers must be shared across crates, put them in the test helper crate instead of gating production source with `#[cfg(test)]`. Do not add or use `#[allow(dead_code)]`; move unused or test-only code into test files instead.
- New integration tests must live in `crates/monochange_integration_tests`, use file fixtures instead of dynamically generated fixtures, and use Insta snapshots for integration output assertions.
- Snapshot readability: JSON snapshots must not embed multiline strings with escaped `\n` sequences. Redact multiline JSON fields as `"[multiline text]"` and add separate string snapshots for the multiline contents.
- Snapshot relevance: `test:cargo` and CI reject unreferenced `.snap` files. Use `snapshot:update` to regenerate snapshots and delete unreferenced snapshot files.

## Naming convention

- The project name is always written in **all lowercase**: `monochange`.
- Never use `MonoChange`, `Monochange`, or `MONOCHANGE` in prose, docs, comments, or string literals.
- Issue titles must use sentence case, must not end with a full stop, and must not use conventional-commit prefixes such as `feat:` or `fix:`.
- Pull request titles must use conventional-commit style prefixes.
- Commit titles must use conventional-commit style prefixes.
- **Rust code exception**: standard Rust naming conventions apply.
  - Structs and enums may use PascalCase (e.g. `MonochangeError`, `MonochangeResult`).
  - Constants use UPPER_SNAKE_CASE (e.g. `MONOCHANGE_VERSION`).
  - Variables and functions use snake_case (e.g. `monochange_config`).

## Git rules

- Always GPG sign commits with `git commit -S` so commits show as verified on GitHub.
- Never use `--no-verify` with `git commit` or `git push`.
- Never change `commit.gpgsign` in local, global, or workspace git config.
- The only allowed exception is during `git rebase` workflows when a rebase continuation or amend step would otherwise block on hooks/editor behavior.

## Agent authority limits

These operations are **strictly prohibited** for the agent and must only be performed by a human maintainer:

- **Never publish any package** to any registry (crates.io, npm, JSR, PyPI, pub.dev, etc.), whether via automated workflow, manual CLI command, or by using local credentials stored on the machine.
- **Never merge a release PR** (e.g. PRs titled `chore(release): prepare release`). These are managed by the automated release workflow and must not be merged by the agent.
- **Never push directly to `main`** or any protected branch. All changes must go through a pull request.
- **Never trigger release or publish workflows** (`release.yml`, `publish.yml`, `docs-release.yml`, etc.) manually.
- **Never use local credentials** (cargo tokens, npm tokens, GitHub tokens, OIDC tokens, etc.) to perform any registry-side operation.
- **Never create, delete, or modify tags or releases** on GitHub.
- **Never add the `no-changeset-required` label** to an issue or pull request. Only Ifiok Jr. may decide that a change does not need a changeset and apply this label.

The agent must only write code, open and update pull requests, review code, run tests, and perform other development tasks. Release and publish operations are the sole responsibility of the human maintainer.

## Release-note streams and changeset writing

- Treat every `.changeset/*.md` file as one audience-specific release-note entry. All targets in that file must resolve to the same changelog stream.
- Types without an explicit `stream` belong to the built-in `default` stream, which is normally developer-facing. Custom streams such as `user` are declared in `monochange.toml` and reached through their configured types.
- When one code change matters to developers and users, write two changesets: one with precise API, function, migration, or operational detail in the default stream, and one with product-language outcomes in the user stream. Do not duplicate the same prose.
- Developer notes should name the affected API or behavior and include actionable migration/config examples. User notes should describe the visible outcome, who benefits, and any action the user must take; omit internal function names and implementation detail.
- For mobile apps, use a configured `native` major type when native binaries or app-store distribution are required. Use a configured `app_feature` minor type for changes eligible for a Shorebird release. The configured type—not subjective wording—must drive this release decision.
- Run `monochange step validate` to reject mixed-stream files, then inspect `monochange step prepare-release --dry-run --format json`. The manifest records `output` and `stream` for every rendered artifact, making the release decision auditable.
- Use `monochange notes --output <id> [--target <package-or-group>]` to inspect one configured artifact without changing versions, changesets, or configured changelog files. Redirect stdout or pass `--file <path>` when CI needs a durable artifact; never scrape a prepared changelog file as a substitute for selecting its named output.

## Task-specific guidance

- [Tooling and commands](docs/agents/tooling.md)
- [Workflow expectations](docs/agents/workflow.md)
- [GitHub templates](docs/agents/github-templates.md)
- [Testing requirements](docs/agents/testing.md)
- [Documentation workflow](docs/agents/documentation.md)
- [Architecture boundaries](docs/agents/architecture.md)
- [Product and architecture rules](docs/agents/product-rules.md)
- [Rust quality and safety](docs/agents/rust-quality.md)
- [Coding style](docs/agents/coding-style.md)
- [Changeset quality](docs/agents/changeset-quality.md)
- [Plans and execution notes](docs/plans/README.md)
