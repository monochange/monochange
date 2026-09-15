# Release-note entry dedup and per-package bump provenance

## Status

- Scope: prepared-release notes, group and package changelogs, release-note entry model
- Outcome: one entry per changeset, placed in its highest-priority section, carrying every package with the bump that package received
- Code changes: `monochange_core`, `monochange_changelog`, integration fixtures

## Problem statement

One changeset can list several targets, and each target carries its own change type and bump. Every target shares the changeset body, so all targets produce entries with identical summary and details text.

`build_release_note_sections` routes each change to a section by its change type and then keeps every routed entry. A changeset that targets one package as `major`, another as `minor`, and a third as `docs` therefore writes the same paragraph into three sections. In a real prepared release this produced the same "split `floats` and `fixed`" block under Breaking Changes, Features, and Notes, each copy listing only the packages that happened to land in that section.

The reader sees one change reported three times, and no copy states which package received which bump.

## Expected behavior

- A change appears once in one document. Its identity is its source changeset plus the summary and details text it renders.
- The surviving entry sits in the highest-priority section among the sections its targets would have used. Lowest configured `priority` value wins, which is the breaking section by default.
- The surviving entry lists every affected package, not only the packages that shared the winning section.
- Each listed package carries the bump that package received, so a reader can tell which package was breaking and which was minor.
- Packages keep a stable first-seen order, and section priority decides the destination deterministically.

## Decisions captured

- Identity is `(source_path, summary, details)`. Entries with no source path are synthesized messages (empty-update and group-fallback text) and are never merged, because two packages legitimately produce similar synthesized text.
- Duplicate resolution happens in `build_release_note_sections`, the only place that knows the configured section priorities. `aggregate_group_release_note_changes` keeps its current job: merging same-key changes across group members so a package list is built at all.
- Priority ordering, not bump ordering, selects the destination. Both are configured, and priority is the field the user already controls.
- An uncategorized change ranks after every configured section, matching the current behavior where the fallback "Changed" section renders last. It is never dropped by the ignored threshold.
- A bump symbol is opt-in per repository through `[changelog.style].package_bump_symbols` and defaults to `true`. `🔴` major, `🟠` minor, `🟢` patch, `⚪` none.
- The entry keeps the change type of the variant that won the section, so `{{ type }}` and the rendered heading agree with the section that shows it.
- `ReleaseNotesEntry.packages` changes from `Vec<String>` to a structured `Vec<ReleaseNotePackage>`. `ReleaseNotesEntry` is not part of a durable artifact schema; the durable `ReleaseNotesDocument<String>` shape is unchanged.

## Ordered checklist

- [ ] Add `ReleaseNotePackage { name, bump }` and a bump-symbol table to `monochange_core`
- [ ] Change `ReleaseNotesEntry.packages` to the structured list
- [ ] Add `ChangelogStyle.package_bump_symbols` with label rendering in Markdown and text
- [ ] Rewrite `build_release_note_sections` to merge by identity and pick the best section
- [ ] Recompute entry style from the merged bump
- [ ] Unit tests for merge, priority, ignored sections, and symbol rendering
- [ ] Integration fixture plus snapshot tests for a multi-target changeset
- [ ] Document the style option and symbol table
- [ ] Developer and user changesets

## Validation commands

```bash
devenv shell cargo test -p monochange_core --lib
devenv shell cargo test -p monochange_changelog --lib
devenv shell cargo test -p monochange_integration_tests
devenv shell lint:all
devenv shell monochange step validate
devenv shell coverage:patch
```

## Risks and boundaries

- Merging across sections changes existing prepared-release and changelog output. Snapshot updates are expected; the release decision itself is unchanged.
- The structured `packages` field changes `--format json` release-note output. Announce it in the developer changeset.
- Symbol rendering must not apply to the durable `ReleaseNotesDocument<String>` text that providers compare for coverage, so `normalized_release_entry` keeps ignoring the `_Packages:_` line.
