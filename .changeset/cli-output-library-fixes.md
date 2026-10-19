---
monochange_core: fix
monochange_github: fix
monochange_lint: fix
---

# Report per-suite lint rule counts and GitHub lookup failures without a config prefix

- `monochange_lint`: `LintProgressReporter::suite_started` now receives the number of the suite's own rules that the current selection allows, instead of every registered rule that applies to the suite's first target. Reporters that print "checking N files with M rules" now show each suite's real rule count.
- `monochange_github`: a failed review-request GraphQL lookup now returns `MonochangeError::Diagnostic` with GitHub's status and message (for example `status 401; Bad credentials` for `POST /graphql`) instead of `MonochangeError::Config("failed to batch load … GitHub")`. Callers that matched the old `config error:` text should match the diagnostic instead.
- `monochange_core`: the built-in `format` step input's help text is now `Output format for the command result`, matching every other `--format` option.
