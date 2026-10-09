---
monochange_npm: fix
---

# Parse pnpm 12 lockfiles written as two YAML documents

pnpm 12 emits `pnpm-lock.yaml` as two YAML documents separated by `---`: an env document holding `configDependencies` and `packageManagerDependencies`, followed by the main project lockfile. `update_pnpm_lock_text` validated the file with a single-document YAML parser, so preparing a release in a pnpm 12 workspace failed with `failed to parse pnpm lock yaml` and the lockfile had to be refreshed manually with `pnpm install --lockfile-only`.

The updater now validates every document in the stream and scans each top-level `importers`, `packages`, and `snapshots` section, so pinned versions rewrite in the project document while the env document, separators, quoting, and formatting stay byte-for-byte identical. `monochange_npm::validate_versioned_file` also accepts multi-document pnpm lockfiles for typed `versioned_files` entries.
