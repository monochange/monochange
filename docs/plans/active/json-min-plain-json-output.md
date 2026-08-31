# Plan: plain-text JSON output and `--format json-min`

## Goal

- All `--format json` CLI output (flags and cli step inputs) renders as plain text: no text colors, no background colors, no ANSI styling of any kind.
- New `--format json-min` choice everywhere `json` is accepted. It emits the same JSON data minified (no indentation, no whitespace between tokens).

## Design

- Add `OutputFormat::JsonMin` (crates/monochange/src/lib.rs).
- Add a single format-aware JSON serializer helper (`OutputFormat::render_json` helper in cli_runtime.rs) that returns pretty JSON for `Json` and minified JSON (`serde_json::to_string`) for `JsonMin`.
- Route every existing `OutputFormat::Json` render site through the helper so JsonMin works identically for free.
- Accept `json-min` in:
  - `parse_output_format` (cli_runtime.rs)
  - every `--format` clap `value_parser` list in cli.rs
  - `CliStepDefinition::valid_input_choices` in monochange_core (`["text", "json", "md"]` for `format`)
  - subagents `SubagentOutputFormat` and sync `VersionsOutputFormat`
  - the repo `monochange.toml` `[cli.release]` format choices
- JSON paths must never style output: verify with integration tests run under `CLICOLOR_FORCE=1` (color-forcing env) plus snapshot tests that no escape sequences appear and minified output parses back to the same data.

## Work items

1. monochange_core: `valid_input_choices` gains `json-min` (unit test).
2. monochange:
   - lib.rs enum + `detect_output_format_from_env_args` + config-step json render + CLI snapshot classification render.
   - cli_runtime.rs `parse_output_format`, `render_json_output` (format-aware), all `resolve_command_output` arms, Discover step, DisplayVersions step.
   - release_artifacts.rs `render_discovery_report` + `render_release_cli_command_json`.
   - analyze.rs, change_classify.rs, release_record.rs, publish_readiness.rs, migration_audit.rs, sync.rs, subagents.rs, lint.rs.
   - cli.rs value_parser lists.
3. Tests: unit tests in `crates/monochange/src/__tests__` + core tests; integration tests in `crates/monochange_integration_tests` with fixtures under `fixtures/tests/` and insta snapshots.
4. Changesets for the `monochange` crate (new `json-min` choice) and note the plain-text guarantee.
5. Validate: devenv, fix:all, test:cargo, coverage:patch, docs touch-ups.

## Coverage plan (100% patch coverage)

- Unit tests for the new variant parsing (accepted + rejected values).
- Unit tests that JSON-min output is minified, valid, and equals the pretty payload when re-parsed, for every touched render site.
- Unit test that covers the serializer error branch via a `Serialize` impl that fails (non-string map key).
- Integration tests: commands run with `--format json-min` snapshot the output; `--format json` snapshots with `CLICOLOR_FORCE=1` prove absence of ANSI.
