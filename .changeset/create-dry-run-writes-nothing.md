---
"monochange": fix
---

# Stop `monochange create --dry-run` from writing the changeset file

`create` (and any configured `[cli.*]` command that binds the `CreateChangeFile` step, such as `monochange run change`) ignored the `--dry-run` flag and wrote the changeset file anyway while printing `wrote change file`, so a preview silently changed release intent for the next `prepare`. Dry-run now performs the same validation and target resolution, prints `would write change file <path>` followed by the rendered changeset content, and writes nothing; the non-dry-run output remains `wrote change file <path>`. The new `monochange::plan_change_file` library API exposes the validated plan (path plus rendered content) without touching the filesystem.
