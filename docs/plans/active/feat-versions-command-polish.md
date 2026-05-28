# Polish the `mc versions` command

## Goal

Make the internal dependency version sync command feel like a first-class migration and maintenance command:

- Rename `mc sync versions` to `mc versions`.
- Preserve the existing Dart and npm sync behavior.
- Add a clearer plan/apply model, JSON output, unsupported ecosystem reporting, and snapshot-tested CLI output.
- Document when users should run it, especially while migrating to monochange or validating that grouped packages have coherent internal dependency constraints.
- Add benchmark coverage so command performance can be tracked over time.

## Implementation checklist

1. [x] Replace the nested `sync versions` command with top-level `versions`.
2. [x] Introduce a version sync plan/result shape that separates discovery/planning from file mutation.
3. [x] Route ecosystem-specific logic through a small adapter abstraction rather than expanding orchestration matches.
4. [x] Add `--format text|json` output with human-readable text as the default.
5. [x] Report unsupported ecosystems explicitly, including skipped file/package counts.
6. [x] Improve conflict/error surfaces by including file paths and ecosystem context in plan/apply failures.
7. [x] Add snapshot-style integration tests for human and JSON CLI output.
8. [x] Add a Criterion benchmark for `mc versions --dry-run --format json` on representative fixtures.
9. [x] Update docs and changesets, including the previous sync-versions changeset text.
10. [x] Create a follow-up issue for version consistency cases that cannot be fixed by dependency constraint syncing alone.

## Performance notes

- Measure release builds/criterion rather than debug timings.
- Avoid reading a manifest twice during non-dry-run apply.
- Preallocate output collections where package counts are known.
- Keep JSON formatting outside hot planning paths.

## Follow-up planning topic

Created follow-up issue: <https://github.com/monochange/monochange/issues/539>

Hard version consistency scenarios include:

- A package in a shared version group already has a higher manifest version than the group’s next planned version.
- Internal dependency constraints point to versions that are valid but incompatible with an upcoming grouped release.
- Packages are migrated into monochange with mixed historical versions and no changeset can infer the intended canonical baseline.
- Ecosystems have lockfiles or workspace-resolution behavior that prevent a simple manifest-only edit from making the workspace consistent.
