---
monochange_analysis: patch
---

# preserve git failure diagnostics during batched revision reads

`monochange_analysis` now waits for `git cat-file` before choosing the error returned by a failed batch read. Callers consistently receive the Git process diagnostic even when the child closes its input pipe before the request writer observes the exit.
