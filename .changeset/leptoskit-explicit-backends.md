---
monochange_app: fix
monochange_extension: fix
---

# Declare backend ownership in each leptoskit application

Both web configurations explicitly select the shared `monochange_app` server. Native and extension configurations explicitly disable backend discovery, so a host-packaging stub cannot be mistaken for an API server. Browser package selection remains in `build.client_package`.

```toml
# Website and projects web configurations
[build]
backend = true

# Native and extension configurations
[build]
backend = false
```
