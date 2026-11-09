---
monochange_app: fix
monochange_extension: fix
---

# Resolve leptoskit from the upstream Git revision

The web app, native workspace, and browser extension now build against one reviewed upstream `leptoskit` commit instead of separate crates.io 0.3.0 copies. This keeps every surface on the same source and lets `cargo update` pick up upstream fixes without waiting for a registry release.

Release operators installing the lint/build CLI must install the same revision, because `cargo install leptoskit_cli` still resolves the stale crates.io release:

```bash
cargo bin --install
# or:
cargo install --git https://github.com/leptoskit/leptoskit \
  --rev a93214edc8e5f6c7999c628285093f15cf188639 --locked leptoskit_cli
```
