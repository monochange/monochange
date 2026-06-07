---
monochange_core: patch
monochange_cargo: patch
monochange_dart: patch
monochange_npm: patch
---

# Add manifest-repository lint rule across all ecosystems

New lint rule that enforces the `repository` field in manifest files (Cargo.toml, pubspec.yaml, package.json) to point to the correct monorepo subdirectory. All rules are Off by default in every preset.

For Cargo, the `cargo/manifest-repository` rule resolves `repository = { workspace = true }` against the root manifest's `workspace.package.repository` (falling back to `package.repository`) and reports a mismatch with an autofix. Set `allow_workspace_inheritance = true` to skip workspace-inherited values instead of resolving them.
