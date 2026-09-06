# `monochange_linting`

<!-- {=monochangeLintingCrateDocs} -->

`monochange_linting` provides authoring helpers and macros for `monochange` lint suites.

Reach for this crate when you are implementing lint rules for an ecosystem adapter and want to reduce declaration boilerplate for metadata, diagnostics, and fixture loading.

## Why use it?

- keep lint rule declarations small with `declare_lint_rule!`
- share `LintRule` construction patterns across ecosystem crates
- focus rule implementations on behavior instead of repeated metadata plumbing

## Best for

- declaring lint rules in ecosystem adapters with minimal boilerplate
- writing lint rule tests with shared snapshot and fixture helpers
- extending the lint pipeline with custom rules that follow the same contract

## Guidance

- Use `declare_lint_rule!` for straightforward rules whose custom behavior mostly lives in `run(...)`.
- The Cargo suite uses the macro for real rules, so the helper reflects actual ecosystem code instead of scaffolding-only examples.
- If a rule eventually needs extra construction state or a custom constructor, an explicit `struct` plus `LintRule::new(...)` is still fine.

<!-- {/monochangeLintingCrateDocs} -->
