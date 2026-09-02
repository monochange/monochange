# Release-note streams

## Problem

Changesets currently feed one developer-oriented changelog. Applications also need public release notes without putting audience metadata or custom syntax in every changeset. A changeset should stay a package target, type, and ordinary Markdown body.

## Scope

- Add a built-in `default` changelog stream that preserves current behavior.
- Allow custom streams to be declared in `monochange.toml`.
- Allow each changelog type to select exactly one stream, defaulting to `default`.
- Reject a changeset whose targets resolve to more than one stream.
- Add named changelog outputs that choose a stream, format, path, and write mode while preserving the current singular changelog configuration.
- Carry stream and output identity through prepared changelogs, release manifests, and release records.
- Document configuration, migration behavior, CLI workflows, and agent-facing changeset guidance.

## Non-goals

- Do not add stream metadata or audience-specific prose fields to changesets.
- Do not allow one target to select several changelog types.
- Do not infer a stream from changed paths or package names.
- Do not change SemVer planning or version allocation.
- Do not merge the pull request without maintainer approval.

## Affected areas

- `crates/monochange_core`: stream/output domain configuration and release artifact identity.
- `crates/monochange_config`: TOML parsing, defaults, and validation.
- `crates/monochange_changelog`: stream filtering and named-output rendering.
- `crates/monochange`: orchestration, templates, release-record plumbing, and CLI-visible diagnostics.
- `crates/monochange_integration_tests` and `fixtures/tests`: realistic release planning scenarios.
- `.templates/`, `docs/`, and the monochange skill: public configuration and changeset authoring guidance.

## Implementation checklist

- [x] Add failing config and release-planning tests for default/custom streams.
- [x] Add failing validation tests for mixed-stream changesets.
- [x] Add failing tests for named outputs and release-record identity.
- [x] Implement domain types and backward-compatible defaults.
- [x] Implement config parsing and validation.
- [x] Filter changes per output stream and preserve existing changelog behavior.
- [x] Update release manifests, release records, provider selection, and snapshots.
- [x] Update templates, schema, guide, skill, and examples.
- [x] Add package-centric changesets with API/config migration examples.
- [x] Reach 100% executable patch coverage.
- [x] Run formatting, lint, build, full tests, docs checks, semantic API checks, affected-package validation, and `monochange step validate`.
- [ ] Open the attributed pull request and wait for all checks to pass.

## Compatibility decisions

- Missing type stream resolves to `default`.
- Existing `[defaults.changelog]`, package changelog, and group changelog values remain valid and produce the same output.
- Existing serialized release artifacts deserialize with `stream = "default"` and an implicit output identity.
- A configured user stream never falls back to content from `default`.
- Empty non-default outputs are omitted unless future configuration explicitly introduces required-output behavior.

## Validation

```bash
devenv shell fix:all
devenv shell build:all
devenv shell lint:all
devenv shell test:all
devenv shell docs:update
devenv shell docs:check
devenv shell coverage:all
devenv shell coverage:patch
devenv shell monochange step validate
```

Run the repository semantic API comparison against `origin/main` after the implementation. Run `cargo semver-checks` for affected published Rust crates if it is available in the development shell; use the result to select major or minor changesets rather than assuming the additive fields are compatible.
