---
"monochange": change
---

# Lead text results with the outcome instead of a completion line

Text results started with ``command `<name>` completed`` and then listed raw fields: a comma-joined `released packages:` line, `- group sdk -> v1.1.0 (tag: true, release: true)`, every changed file, the same files again as the release commit's `tracked paths`, and the full stdout of every command step. The release pull request job printed more than 150 lines after its progress output.

Text results now open with a headline that answers the command's question, followed by titled sections with aligned columns. Long lists stop after 20 entries and point at `--format json`, which keeps the complete data. Colour is used only when stdout is an interactive terminal. The structure of `--format json` and `--format json-min` output is unchanged.

```bash
monochange preview
```

```text
• Release preview · 2 packages · dry-run, no files were changed

Releases
  sdk  v1.1.0  group · tag · release

Packages (2)
  workflow-app, workflow-core

Changed files (6)
  Cargo.toml
  …

Manifest  .monochange/local/release-manifest.json
```

The same treatment applies to:

- `monochange next`: configured package ids instead of record ids such as `cargo:crates/core/Cargo.toml`, with group members listed under their group
- `monochange check`: `✔ Checks passed` or `✖ Checks failed · 9 errors · 7 fixable`, findings grouped by relative file path and sorted by line, rule ids on their own line, and one `monochange check --fix` hint instead of four summaries
- `monochange discover`: ecosystems as a sorted table and warnings with relative paths
- `monochange diagnose`: a one-line preview of each changeset's details, an aligned targets table, and provenance on one line
- `monochange affected`: the policy verdict first, with duplicate warnings removed
- `monochange versions sync`: updates grouped by relative file, and `already in sync` instead of no output
- `monochange step validate`: `✔ Workspace validation passed`
- `monochange create`: `✔ Created changeset <path>`, or `Would create changeset <path>` with the rendered file for `--dry-run`
- release commits: the short SHA and subject with a tracked-path count, instead of every tracked path
- command step logs: each command's last output line, instead of its whole stdout
