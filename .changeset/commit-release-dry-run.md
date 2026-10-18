---
monochange: fix
---

# Preview prepare-and-commit release workflows with --dry-run

`PrepareRelease` skips writing the release record during dry-run previews, but `CommitRelease` still required that record on disk, so `monochange run release
--dry-run` — the natural way to preview a prepare-and-commit pipeline — failed with `no release record found`.

CommitRelease now reports the commit it would make (subject, tracked paths, and the expected `.monochange/releases/<hash>/release.json` path) without requiring or writing the record. The real run still validates and stages the record as before.
