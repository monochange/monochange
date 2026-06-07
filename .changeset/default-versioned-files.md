---
monochange: patch
monochange_config: patch
monochange_core: patch
monochange_schema: patch
---

# Honor default versioned files during release preparation

Workspace-level `versioned_files` defaults now apply to manually configured and auto-discovered packages. Release preparation now ignores missing formatted fields by default, and `missing_field_behavior = "add"` can be used for shared version files that should create missing package entries.

For example, this shared `versions.json` file now creates missing package keys during release preparation:

```toml
[defaults]
package_type = "npm"
versioned_files = [
	{ path = "versions.json", format = "json", fields = ["packages.{{ name }}"], missing_field_behavior = "add" },
]

[package.app]
path = "packages/app"
```

When `versions.json` starts as:

```json
{
	"packages": {}
}
```

Releasing `app` to `1.2.3` updates it to:

```json
{
	"packages": {
		"app": "1.2.3"
	}
}
```
