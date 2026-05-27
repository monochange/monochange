---
monochange: fix
---

# Support `--version` flag on mc CLI

The root clap command was missing `.version()`, causing `mc --version` to be rejected as an unexpected argument. Added `CARGO_PKG_VERSION` registration so the flag now works correctly.
