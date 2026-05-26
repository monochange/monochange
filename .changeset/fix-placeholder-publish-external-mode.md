---
monochange: patch
monochange_core: patch
monochange_publish: patch
---

# Fix placeholder publish skipping external-mode packages

Previously, `mc step:placeholder-publish` skipped packages configured with `publish.mode = "external"`, showing messages like "package opted out of built-in publishing". This was incorrect because placeholder publishing is a bootstrap utility separate from normal release publishing.

Now placeholder publishing proceeds for all publishable packages regardless of `publish.mode`. The following safeguards remain in effect:

- `publish.enabled = false` still opts out completely
- Private/unpublishable package metadata is still respected
- Registry support limitations are still enforced
