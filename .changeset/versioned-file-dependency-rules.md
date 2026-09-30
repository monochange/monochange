---
monochange: patch
monochange_core: minor
---

# Preserve explicit dependency rules during release preparation

Preparing a release now preserves configured dependency prefixes after native manifest synchronization, and lock commands see the final manifest contents. Dependency-only npm versioned entries leave the file's own version unchanged; scalar dependency fields still update their selected constraints.

```toml
[package.api]
path = "packages/api"
type = "npm"
versioned_files = [
	{ path = "packages/ui/constraints.json", type = "npm", name = "api", fields = ["dependencies"], prefix = "~" },
]
```

The explicit `name` refers to a configured package ID and resolves to that package's native dependency name. When `api` releases, this entry updates matching constraints using `~` while preserving `constraints.json`'s own version.

Library callers can use the additive `monochange_core::update_selected_json_manifest_text(contents, owner_version, fields, versioned_deps)` API to update only selected fields. Nested owner fields such as `metadata.version` still receive the owner version without implicitly changing the root `version`. The existing `update_json_manifest_text` API retains its implicit native-manifest root-version behavior.
