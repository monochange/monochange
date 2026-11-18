---
monochange_app: fix
---

# Enforce SDK initial-load budgets before publishing web assets

Both web frontends now set the SDK's Brotli initial-load budget, so `build:web` and the deployment image stop before publication when either entry exceeds its limit. The measured initial loads leave approximately 5% headroom. The existing whole-site, per-file and raw-asset gates remain unchanged.

Website (`app/leptoskit.toml`):

```toml
[bundle]
budget_kib = 315
```

Projects (`app/projects/leptoskit.toml`):

```toml
[bundle]
budget_kib = 302
```
