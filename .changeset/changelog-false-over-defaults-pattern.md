---
"monochange_config": patch
---

# Honor `changelog = false` on a package that also inherits a changelog default

A package-level `changelog = false` was ignored whenever `[defaults.changelog]` configured a path pattern such as `"{{ path }}/changelog.md"`. `resolve_for_package` returns `None` both for a disabled definition and for one it cannot resolve, and the package resolver treated every `None` as "fall back to the default", so the inherited pattern was applied anyway:

```toml
[defaults]
package_type = "cargo"
changelog = "{{ path }}/changelog.md"

[package.opted-out]
path = "crates/opted-out"
changelog = false # previously ignored
```

This made a package unable to opt out of an inherited changelog. In a repository where a version group also renders its changelog to that package's default path, the two owners collided and release planning failed with:

```text
changelog outputs `default` and `default` both render to `crates/opted-out/changelog.md`; configure unique output paths
```

The resolver now consults the existing disabled check before falling back to the workspace default, matching how group changelog definitions are already resolved. `changelog = false` on a package disables its changelog even when a defaults pattern is configured, and the default still applies to packages that do not declare their own changelog.
