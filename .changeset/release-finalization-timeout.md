---
monochange_app: patch
---

# Allow release finalization to finish its cache cleanup

Increase `release-post-merge.timeout-minutes` in `.github/workflows/ci.yml` from `10` to `20`. Pinned tooling setup, CLI compilation, release operations, and cache cleanup share this job deadline. The previous limit canceled release finalization while saving the Rust cache, after tags, draft releases, and the publish dispatch had succeeded. The larger budget leaves time for cleanup without changing release commands, permissions, or deployment checks.
