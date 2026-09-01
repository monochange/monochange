# Type-scoped changeset lints

## Problem

`[lints.rules]."changesets/types/<type>"` is documented and parsed, but the manifest-lint registry only created fixed bump-scoped runners. As a result, type-scoped requirements such as custom release-note sections did not run during `monochange check`.

## Scope

- Register one changeset lint runner for each configured changelog type.
- Apply the existing scoped options only to matching changeset entries.
- Cover the public `monochange check` path with a fixture-first regression test.
- Preserve the existing static lint catalog and bump-scoped behavior.

## Completed

- [x] Added a failing CLI regression fixture for `changesets/types/app_feature`.
- [x] Added config-aware construction for the changeset lint suite.
- [x] Implemented the type-scoped runner using the shared scoped-rule options.
- [x] Added focused unit coverage for matching, non-matching, disabled, and invalid parsed-target paths.
- [x] Added a package changeset that documents the before/after configuration.
- [x] Ran formatting, focused tests, the full workspace build/test suite, workspace validation, affected-package verification, and dependency policy validation.

## Non-goals

- Custom changelog type parsing and release-note section routing are unchanged.
- Wildcard matching was not added to the shared lint registry.
- Existing bump-scoped lint behavior is unchanged.
