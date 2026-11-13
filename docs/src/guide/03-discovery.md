# Discovery

`monochange` discovers packages from native manifests and workspace definitions. For a capability-by-capability comparison of each adapter, see [Ecosystems](./ecosystems.md).

Supported sources:

<!-- {=discoverySupportedSources} -->

- Cargo workspaces and standalone crates
- npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages
- Deno workspaces and standalone `deno.json` / `deno.jsonc` packages
- Dart and Flutter workspaces plus standalone `pubspec.yaml` packages
- Python uv workspaces, Poetry projects, and standalone `pyproject.toml` packages
- Go modules discovered from standalone `go.mod` files

<!-- {/discoverySupportedSources} -->

Run discovery:

<!-- {=projectDiscoverCommand} -->

```bash
monochange discover
```

<!-- {/projectDiscoverCommand} -->

Key behaviors:

<!-- {=discoveryKeyBehaviors} -->

- native workspace globs are expanded by each ecosystem adapter
- dependency names are normalized into one graph
- package ids and manifest paths in CLI output are rendered relative to the repository root for deterministic automation
- gitignored paths and nested git worktrees are skipped during discovery
- version-group assignments are attached after discovery
- unmatched group members (declared in config but not found during discovery) produce warnings
- unresolvable group members (invalid package IDs in `group.packages`) produce errors during configuration loading
- discovery scans all supported ecosystems regardless of `[ecosystems.*]` toggles in `monochange.toml`

<!-- {/discoveryKeyBehaviors} -->

## Configured package IDs

Different ecosystems can publish the same package name. Give each package a distinct ID in `monochange.toml`:

```toml
[package.shared]
path = "crates/shared"

[package."dart:shared"]
path = "packages/shared"
type = "dart"
```

An exact configured ID takes precedence over published names. In changesets and version groups, `shared` selects the Cargo package and `dart:shared` selects the Dart package. References that do not match an explicit configured ID still resolve by published name, canonical ID, or manifest/directory path; an ambiguous published name remains an error.
