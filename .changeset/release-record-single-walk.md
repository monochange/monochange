---
monochange: perf
---

# Resolve release records with one git walk instead of several per commit

Release-record discovery — the lookup behind `step release-record`, `publish-readiness`, `tag-release`, and retarget planning — walked first-parent history by running four to six git commands per commit: a commit inspection for the parent, a redundant `rev-parse`, a shallow-file lookup, a parent-existence probe, and then the diff itself.

The walk now resolves every commit and its first parent in one `git log --first-parent --format=%H %P` run. A commit whose first parent never appears in the walk sits at a cut boundary (the root, or a shallow clone), and only those fall back to a full tree listing — the same fallback the per-commit probes used to select, decided from the single walk instead of extra git calls. Each non-boundary commit now costs one `git diff-tree` instead of the previous four to six processes, so finding a record hundreds of commits back stops spawning thousands of git processes.

Release-commit staging also asks git about every path at once: one `git ls-files` call reports which paths are tracked, and one `git check-ignore` call reports which are ignored, replacing up to two git processes per release path.
