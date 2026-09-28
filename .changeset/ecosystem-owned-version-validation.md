---
"monochange": patch
"monochange_config": major
"monochange_cargo": patch
"monochange_npm": patch
"monochange_deno": patch
"monochange_dart": major
---

# Versioned file validation dispatches through the ecosystem registry

`monochange_config` parsed TOML, JSON, and YAML itself to decide whether a configured `versioned_files` entry pointed at a readable version field, duplicating every ecosystem's format knowledge inside the config crate. That validation now dispatches through the `EcosystemRegistry` from `monochange_core`, so each ecosystem adapter owns parsing for its own manifest format and the config crate only adds the owning package or group to the resulting error.

`validate_versioned_files_content` and `validate_versioned_files_content_with_config` take the registry as a third argument. Callers that invoke them directly must now pass one:

```rust
// before
monochange_config::validate_versioned_files_content_with_config(root, &configuration)?;

// after
let ecosystems = monochange_core::EcosystemRegistry::new();
ecosystems.push_adapter(Box::new(monochange_cargo::adapter()));
monochange_config::validate_versioned_files_content_with_config(root, &configuration, &ecosystems)?;
```

The CLI builds the registry from its enabled ecosystem features, so `monochange check` behavior is unchanged apart from the fixes below.

## Two validation bugs fixed

Both previously made `monochange check` reject configurations that `monochange prepare` accepts:

- Custom `fields` entries that address a nested path, such as `metadata.bin.monochange.version` in `package.json`, were compared against the root object as a literal key and always failed.
- Only the first entry of `fields` was checked, so a typo in a second or later field passed validation.

A `versioned_files` entry with `fields = ["version", "dependencies"]` now validates both entries, and a Dart dependency section resolves instead of being read as a string. Cargo keeps accepting any of `package.version`, `workspace.package.version`, or `version` when `fields` is unset. Python and Go files keep skipping field validation: a `pyproject.toml` may derive its version dynamically, and a module's version comes from its git tag rather than `go.mod`.

`monochange_dart` renames the third parameter of `validate_versioned_file` from `_custom_fields` to `custom_fields`, which the API classifier records as a modified public item; the `major` bump reflects that signature change even though callers pass the same arguments.
