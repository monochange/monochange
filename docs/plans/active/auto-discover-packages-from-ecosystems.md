# Auto-discover packages from ecosystem configuration

**Status**: Draft\
**Priority**: Medium\
**PR scope**: `monochange_core`, `monochange_config`, `monochange`, integration tests

## Problem

Declaring 30+ packages one-by-one in `monochange.toml` is repetitive. Most packages only need `path` and inherit everything else from `[defaults]`. In the monochange repo itself, all 35 `[package.*]` entries are boilerplate: 21 Cargo packages and 14 npm packages, most with only `path = "..."`.

## Proposed API

Add `auto_discover` to each `[ecosystems.*]` section. When enabled, monochange walks the workspace looking for ecosystem manifests (Cargo.toml, package.json, etc.) in paths matching the `include` glob patterns. Discovered packages get their ID from the manifest's `name` field and inherit ecosystem-level defaults. Explicit `[package.*]` entries override auto-discovered defaults.

```toml
[ecosystems.cargo]
enabled = true
versioned_files = ["Cargo.toml"]
auto_discover = { include = ["crates/*"] }

[ecosystems.npm]
enabled = true
auto_discover = { include = ["packages/*"] }

# Only packages that deviate from defaults need explicit entries:
[package.monochange_schema]
path = "crates/monochange_schema"
release = true
tag = true
```

### Auto-discover settings

```toml
[ecosystems.cargo]
# include: glob patterns for directories to scan (required)
# exclude: glob patterns to skip within included paths (optional)
# id: optional package id template (default: "{{ name }}")
# defaults: package-level defaults for all auto-discovered packages (optional)
auto_discover = { include = ["crates/*"] }
auto_discover = { include = ["crates/*"], exclude = ["crates/monochange_test_helpers"] }
auto_discover = { include = ["crates/*"], id = "cargo:{{ name }}" }
auto_discover = { include = ["crates/*"], defaults = { tag = true } }
```

### Precedence

```
[package.*] explicit  >  [ecosystems.*].auto_discover.defaults  >  [defaults]
```

When a package is both auto-discovered and explicitly declared, the explicit `[package.*]` entry wins for every field it sets. Fields not set in the explicit entry fall back to `ecosystem auto_discover.defaults`, then `[defaults]`.

### id templates

| Template             | Behavior                                                                                              |
| -------------------- | ----------------------------------------------------------------------------------------------------- |
| `"{{ name }}"`       | Use the manifest package name, falling back to the discovered directory basename when no name exists. |
| `"cargo:{{ name }}"` | Prefix IDs when multiple ecosystems can contain packages with the same manifest name.                 |
| `"{{ path }}"`       | Use the package directory path relative to the workspace root.                                        |

The `{{ name }}` default means auto-discovered packages use the same canonical id that ecosystem tooling already knows. Custom templates are useful when multiple ecosystems can share package names or when a workspace wants path-derived IDs.

### Conflict handling

- If auto-discovery finds a package whose id collides with an explicit `[package.*]`, the explicit entry wins and no warning is emitted.
- If two auto-discovered packages (from different ecosystems) produce the same id, validation emits an error: monochange cannot determine which ecosystem owns the id. This is extremely rare since Cargo uses `my-crate` and npm uses `@scope/my-crate`.
- If an auto-discovered package id collides with a `[group.*]` id, validation emits an error (package and group ids share one namespace).

### Interaction with existing features

- **Groups**: Auto-discovered packages can be referenced in `[group.*].packages` by their id just like explicit packages.
- **Discovery**: Auto-discovery produces declared packages. The `[ecosystems.*]` discovery step (which finds packages by walking filesystem manifests) still runs, but its output is merged with auto-discovered declarations.
- **Validation**: `monochange step validate` will warn about glob patterns that match zero directories and error on ambiguous overlaps.
- **`monochange init`**: Can generate `auto_discover` settings by detecting the repo layout, eliminating the need for most `[package.*]` entries.

## Implementation plan

### Phase 1: Core types and config parsing

1. **Add `RawAutoDiscoverSettings` in `monochange_config`**:
   ```rust
   pub(crate) struct RawAutoDiscoverSettings {
   	include: Vec<String>, // glob patterns
   	exclude: Vec<String>, // glob patterns to skip
   	id: Option<String>,   // template, default: "{{ name }}"
   	defaults: Option<RawAutoDiscoverPackageDefaults>,
   }
   ```

2. **Add `auto_discover` field to `RawEcosystemSettings`**:
   ```rust
   pub(crate) struct RawEcosystemSettings {
   	// ... existing fields ...
   	auto_discover: Option<RawAutoDiscoverSettings>,
   }
   ```

3. **Add `AutoDiscoverSettings` and `AutoDiscoverPackageDefaults` to `monochange_core`**: Normalized, validated versions of the raw types.

4. **Add `auto_discover` field to `EcosystemSettings`**:
   ```rust
   pub struct EcosystemSettings {
   	// ... existing fields ...
   	pub auto_discover: Option<AutoDiscoverSettings>,
   }
   ```

### Phase 2: Discovery walk

5. **Implement `discover_packages_from_ecosystem()`** in `monochange_config`**:
   - Walk directories matching `include` globs, skip `exclude` globs
   - For each matching directory, look for the ecosystem's manifest file (Cargo.toml, package.json, pubspec.yaml, etc.)
   - Render the package ID template from manifest and path context
   - Return a list of `(id, path, ecosystem_type)` tuples

6. **Merge auto-discovered packages into the package map**:
   - After loading explicit `[package.*]` entries, run auto-discovery
   - For each discovered package not already in the map, create a `PackageDefinition` using ecosystem defaults + `auto_discover.defaults`
   - For discovered packages that match an explicit `[package.*]`, the explicit entry already overrides (no action needed)

### Phase 3: Validation and diagnostics

7. **Validate auto-discover config**:
   - Warn on glob patterns that match zero directories
   - Error on id collisions between auto-discovered packages
   - Error on id collisions between auto-discovered and group ids

8. **Update `monochange step validate`** to report auto-discovered packages

### Phase 4: Template and docs

9. **Update `crates/monochange/src/monochange.toml.template`** to document the new `auto_discover` option in ecosystem sections.

10. **Update `docs/src/reference/` documentation**.

### Phase 5: Integration tests

11. **Add file fixtures** for auto-discovered packages:
    - Cargo workspace with `crates/*` auto-discovery
    - npm packages with `packages/*` auto-discovery
    - Mixed ecosystem with overlapping globs
    - Explicit overrides of auto-discovered packages
    - Conflict detection (same id from two ecosystems)

## Open questions

- Should `auto_discover` also apply to `[ecosystems.dart]` and `[ecosystems.go]`? Yes: the same mechanism works for all ecosystems.
- Should `auto_discover.include` support `**` for recursive matching? Yes: use the `glob` crate which already supports `**`.
- Should `auto_discover` be a boolean shorthand? E.g. `auto_discover = true` uses `[ecosystems.*].roots` or the workspace root as the include pattern. Decision: start with the explicit table form only; add shorthand later if users request it.
- How does `auto_discover` interact with `[ecosystems.*].roots` and `exclude`? If `roots` is set, auto-discovery is restricted to those root directories. If `exclude` is set, matching paths are excluded from both filesystem discovery and auto-discovery.

## NOT in scope

- **Package glob patterns in `[package.*]`** (e.g. `[package."crates/*"]`): This is a separate feature that could be added later but is not part of this proposal. Auto-discovery from ecosystem config is more ergonomic because the ecosystem already knows how to identify its manifests.

- **`[template.*]` sections**: Templates add indirection. Defaults + ecosystem auto-discover already provide the deduplication benefit without an extra concept.

- **Auto-creating `[group.*]` entries**: Groups still need explicit declaration. Auto-discovery only creates packages, not groups. Grouping requires human intent.
