---
monochange_app: fix
---

# Keep navigation verification browser cleanup bounded

The navigation verifier writes its owned Chrome diagnostics to a temporary file. Browser helpers that inherit stderr no longer retain a pipe in the verifier after Chrome exits. Process-exit checks, cleanup deadlines and failure reporting remain enforced.
