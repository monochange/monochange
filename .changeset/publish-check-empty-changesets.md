---
"monochange": patch
---

# Allow publish-check with no pending changesets

The repository `publish-check` workflow now treats an empty changeset directory as a valid dry-run input while still running the package publish validation step.
