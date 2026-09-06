# `monochange_lint`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_lint"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**lint-orange?logo=rust)](https://crates.io/crates/monochange_lint) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**lint-1f425f?logo=docs.rs)](https://docs.rs/monochange_lint/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_lint)](https://codecov.io/gh/monochange/monochange?flag=monochange_lint) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeLintCrateDocs} -->

`monochange_lint` is the ecosystem-agnostic manifest lint engine for monochange.

Reach for this crate when you want to validate workspace manifests against registered lint suites, rules, and presets without coupling the engine to specific ecosystems.

## Why use it?

- run one lint engine across every registered ecosystem suite
- keep the engine unaware of which ecosystems exist; ecosystem crates contribute suites, rules, presets, and parsed lint targets
- resolve rule and preset configuration through one registry

## Best for

- enforcing manifest quality checks across multi-ecosystem monorepos
- building custom lint suites that plug into the shared lint pipeline
- applying the shared lint pipeline from CLI, CI, and automation surfaces

## Public entry points

- `Linter` drives lint execution over registered suites
- `lint_workspace(workspace_root, configuration, reporter)` lints all suite targets in the workspace
- `LintSelection` narrows suites and rules; `LintRegistry` resolves rules and presets

<!-- {/monochangeLintCrateDocs} -->
