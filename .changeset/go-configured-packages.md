---
"monochange_core": minor
"monochange_go": minor
---

# Load configured Go modules as tag-versioned packages

`Ecosystem::versions_from_tags` marks ecosystems whose released versions are identified by git tags instead of a manifest field, and the Go adapter now implements `load_configured` through the new public `load_configured_go_package(root, package_path)`. Configured `type = "go"` packages therefore resolve a `PackageRecord` with no `current_version`; release planning owns the tag-based baseline.
