# Generic format-based versioned files

## Status

- Branch/worktree: `feat/versioned-file-formats` at `worktrees/feat-versioned-file-formats`
- Owner: Uche
- Started: 2026-05-30

## Problem statement

`versioned_files` currently has two modes:

1. Ecosystem-aware mode via `type = "cargo"`, `type = "npm"`, `type = "dart"`, etc. The ecosystem type controls which file kinds are supported and which dependency/version fields are updated by default.
2. Regex replacement mode via `regex`, for raw text replacement with a named `version` capture.

This leaves no first-class way to update arbitrary structured or simple key/value files such as custom JSON metadata, TOML config, YAML config, or `.env` files without pretending they are an ecosystem manifest or writing a regex.

## Desired user-facing shape

Add a mutually exclusive generic format mode for versioned files:

```toml
versioned_files = [
	{ path = "metadata.json", format = "json", fields = ["release.version"] },
	{ path = ".env", format = "env", fields = ["VERSION"] },
	{ path = "tools.toml", format = "toml", fields = ["tool.my_package.version"] },
]
```

Rules:

- `format` is mutually exclusive with ecosystem `type`.
- `format` is mutually exclusive with `regex` replacement mode.
- `format` requires at least one explicit field.
- `format` does not infer dependency sections or ecosystem-specific behavior.
- Each configured field is updated to the planned version for the owning package/group.
- Field paths should be deterministic and easy to document:
  - JSON/TOML/YAML: dot-separated path segments like `x.y.z` for object/table keys.
  - env: exact key names like `VERSION`.
- Templating can be layered in after the basic feature is reliable. Initial implementation should reserve the design and avoid blocking future support for `{{ name }}` / `{{ version }}` style templates.

## Scope

### In scope for the first implementation

- Add shared config/domain types for generic versioned file formats.
- Support `format = "json"`, `format = "toml"`, `format = "yaml"`, `format = "yml"`, and `format = "env"`.
- Validate invalid combinations and missing fields in `monochange_config`.
- Apply version updates in `crates/monochange/src/versioned_files.rs`.
- Preserve existing ecosystem `type` and `regex` behavior.
- Add fixture-first tests for config loading and release/update behavior.
- Update schema/docs/templates/config comments and add a changeset.

### Non-goals for the first implementation

- Generic dependency-aware updates by package name for arbitrary formats.
- Wildcard paths, array indexing, JSONPath/JMESPath, or recursive search.
- Creating missing nested fields unless the existing codebase already has a clear precedent. Prefer requiring configured fields to exist for safety.
- Rich value templating beyond writing the planned version string. Keep the model extensible for a later `value`/`template` field.
- Lossless preservation for every YAML/TOML nuance if existing serializers cannot guarantee it; document current behavior and add snapshots for the chosen output.

## Affected areas

- `crates/monochange_core/src/lib.rs`
  - `VersionedFileDefinition`
  - new `VersionedFileFormat` enum if appropriate
- `crates/monochange_config/src/lib.rs`
  - normalize and validate versioned file definitions
  - schema generation annotations
- `crates/monochange_config/src/__tests__/lib_tests.rs`
  - config validation tests
- `crates/monochange/src/versioned_files.rs`
  - generic format mutation logic
  - cached document handling for JSON/YAML/TOML/env/text
- `crates/monochange/src/__tests__/versioned_files_tests.rs`
  - release/update behavior tests
- `fixtures/tests/...`
  - fixture-first scenarios for generic format files
- Documentation/config artifacts
  - `monochange.toml` comments/examples
  - docs reference pages for config/versioned files
  - generated schema snapshots/artifacts if affected
- `.changeset/*.md`
  - user-facing release note for new config feature

## Design notes and open decisions

- Use `format` rather than `formats`; each versioned file entry describes one concrete file format. If the public API must match the prompt's plural wording, revisit before implementation.
- Reuse existing `fields` for generic mode so the config remains compact. In ecosystem mode, fields keep their existing ecosystem-specific meaning.
- Prefer a typed enum in core (`VersionedFileFormat`) over free strings to keep validation, schema, and CLI JSON stable.
- Treat `yaml` and `yml` as aliases during deserialization, but emit one canonical value if serialized.
- For env files, update only existing `KEY=value` / `export KEY=value` style entries at first. Preserve comments and unrelated lines.
- For JSON/TOML/YAML, require object/table traversal. If a segment is missing or points at a non-container, produce a diagnostic that names the path and field.
- String values should be written as strings. If an existing field is numeric or boolean, replace it with a string version because semver versions are strings.
- Templating follow-up: model could later add `value = "{{ version }}"` and field templates with context `{ id, name, version, owner_kind }`.

## Execution checklist

- [x] Create isolated worktree and branch.
- [x] Capture this implementation plan under `docs/plans/active/`.
- [x] Inspect current versioned file validation/update flow in detail.
- [x] Add failing config tests for accepted generic formats and rejected invalid combinations.
- [x] Add failing versioned file update tests with fixture files for JSON, TOML, YAML, and env.
- [x] Add core types/config fields for generic formats.
- [x] Implement validation rules for `format` mode.
- [x] Implement generic format update dispatch.
- [x] Implement JSON field-path mutation.
- [x] Implement TOML field-path mutation.
- [x] Implement YAML field-path mutation.
- [x] Implement env key mutation.
- [x] Update docs/config examples/schema artifacts.
- [x] Add a changeset describing the new `versioned_files.format` mode.
- [x] Run targeted tests.
- [ ] Run `fix:all`.
- [ ] Run `monochange step validate`, relevant lint/build checks, and patch coverage.
- [x] Mark completed steps and record any deferred follow-ups.

## Execution notes

- Targeted tests passed for `monochange_core`, `monochange_config`, and `monochange` versioned file coverage.
- `cargo clippy --package monochange --lib --all-targets -- -D warnings` passed after addressing format-mode warnings.
- `cargo xtask schema check` passed after regenerating the monochange config schema.
- `monochange step validate` passed.
- `build:all` passed.
- `fix:all` was attempted; it completed formatting/schema/config validation stages but failed in the existing GitHub Actions audit (`zizmor`) on pre-existing workflow findings unrelated to `versioned_files.format`.

## Validation plan

Targeted while developing:

```nushell
devenv shell test:cargo --package monochange_config versioned_file
devenv shell test:cargo --package monochange versioned_files
```

Final local validation:

```nushell
devenv shell fix:all
devenv shell monochange step validate
devenv shell lint:all
devenv shell build:all
devenv shell coverage:patch
```

## Risks

- Format-preserving writes can be tricky, especially for TOML/YAML comments. Keep output explicit in snapshots.
- Existing `fields` semantics may be ecosystem-specific in some adapters. Generic mode must not alter ecosystem behavior.
- Schema updates may affect generated artifacts beyond the immediate Rust crates.
- Regex mode currently allows type-less entries; new validation must keep regex compatibility intact.
