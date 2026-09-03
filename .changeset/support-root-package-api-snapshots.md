---
monochange_analysis: patch
---

# Support API snapshots for workspace-root packages

Use `.` as the Git pathspec when a package manifest lives at the workspace root, allowing affected-package and semantic API checks to compare those packages against a Git revision.
