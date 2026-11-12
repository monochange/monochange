---
monochange_app: fix
---

# Reduce shared stylesheet transfer overhead

The web build drops unused selectors for the retired navigation layout and release-table icon wrappers from the shared stylesheet. Rebuild both browser entries to deploy the smaller CSS asset through the existing static-asset compression pipeline.
