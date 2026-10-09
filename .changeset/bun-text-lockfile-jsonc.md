---
monochange_npm: fix
---

# Rewrite real Bun JSONC lockfiles without corrupting workspace references

`update_bun_lock` replaced the first `"name": "..."` string it found, which corrupted real `bun.lock` files: inter-workspace references such as `"ui": "workspace:packages/ui"` were overwritten with the released version. Bun writes `bun.lock` as JSONC with trailing commas and pins registry dependencies as exact versions.

The updater now rewrites only exact semver pins (`"ui": "1.3.0"`) and the `name@version` descriptors that open `"name": [...]` package-map entries of released packages. Workspace, alias, `npm:` protocol, range, and git or URL references are left untouched, as is the JSONC formatting.
