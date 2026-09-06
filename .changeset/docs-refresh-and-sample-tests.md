---
"@monochange/cli": patch
"@monochange/skill": patch
monochange: patch
monochange_analysis: patch
monochange_cargo: patch
monochange_config: patch
monochange_core: patch
monochange_dart: patch
monochange_deno: patch
monochange_forgejo: patch
monochange_gitea: patch
monochange_github: patch
monochange_gitlab: patch
monochange_go: patch
monochange_graph: patch
monochange_hosting: patch
monochange_lint: patch
monochange_linting: patch
monochange_npm: patch
monochange_python: patch
monochange_semver: patch
monochange_telemetry: patch
monochange_test_helpers: patch
---

# refresh docs and validate doc samples in tests

Every crate's crate-level docs (lib.rs doc comments) now come from the same shared mdt block that feeds its readme, so the two surfaces can no longer drift. Crate docs that had fallen behind the code were rewritten: `monochange_analysis` documents the semantic-analyzer architecture, `monochange_lint` documents the real `Linter`/`lint_workspace` API instead of removed entry points, `monochange_linting` carries the authoring guidance, `monochange_graph` documents the current `build_release_plan` signature with `bump_propagation`, and the `monochange_go`/`monochange_python` intros match the actual adapters.

Documentation samples that declare a complete `monochange.toml` are now validated in tests against the real configuration loader, which caught and fixed several stale samples: the removed `[release_notes]`/`change_templates`/`extra_changelog_sections` options were replaced with the current `[changelog]` API, knope-migration samples no longer use the unsupported `dependency` versioned-file syntax, and `monochange step placeholder-publish` invocations dropped the removed `--from`/`--output` flags.

Command references now distinguish `monochange versions sync` from the read-only `versions list` (and mention the deprecation of bare `monochange versions`), the `publish-packages` reference documents `--all`, `--stream-output`, and `--fail-on-duplicate`, the `comment-released-issues` reference documents `--from-ref` and `--auto-close-issues`, the retarget-release reference no longer documents a `format` input the step does not accept, and the knope-migration and `init --provider` claims now match what those commands actually generate. The skill gained the type-scoped `changesets/types/<type>` lint rules from the linting reference.

New doc-sample validation tests in `monochange_integration_tests` (`docs_code_samples.rs`) parse every fenced `toml` sample in the guide through `load_workspace_configuration`, so documentation samples fail CI when the configuration surface changes.
