# Internal dependency versions

Use `monochange versions sync` to keep internal workspace dependency constraints aligned with each package's canonical version. It is useful when migrating an existing monorepo to monochange and when package versions change during normal release work.

```sh
monochange versions sync --dry-run
monochange versions sync
```

The `monochange versions list` subcommand prints the discovered package and group version inventory without touching files:

```sh
monochange versions list
monochange versions list --format json-min
```

The command scans discovered workspace packages, builds a package-name to version map, and updates supported manifest files where one workspace package depends on another. It only syncs internal workspace dependencies; it does not change external dependency constraints.

## Output

By default, `monochange versions sync --dry-run` prints the file and dependency updates it would make:

```text
would update ^1.1.0 → ^1.2.3 in core (packages/app/pubspec.yaml)

Strategy: default (package config → ecosystem config → ecosystem default; --strategy overrides)

(dry run — no files were modified)
```

Use JSON output for scripts and CI checks:

```sh
monochange versions sync --dry-run --format json
```

The JSON result includes whether changes were applied, the selected strategy, changed files, dependency updates, and any packages skipped during planning.

## Constraint styles and prefixes

`--strategy` controls the constraint prefix written for updated dependencies:

- `default` keeps each ecosystem's own constraint style (see the table below).
- `exact` writes the bare version with no range prefix, such as `1.2.3`.
- `caret` writes a caret constraint, such as `^1.2.3`, for ecosystems that use caret ranges.
- `compatible` writes a compatible-range constraint, such as `>=1.2.3`.

Passing `--strategy` overrides the built-in style for the whole command. `monochange.toml` cannot currently change the style `versions sync` uses — `--strategy` is the only override — and the command always writes the same prefix for a given strategy and ecosystem. The `dependency_version_prefix` ecosystem setting affects versioned-file writes (see [Versioned files](../guide/04-configuration.md#versioned-files)), not `versions sync`.

What each ecosystem receives for an internal dependency on a package at `1.2.3`:

| Strategy     | Cargo     | npm       | Deno     | Dart      | Python    | Go         |
| ------------ | --------- | --------- | -------- | --------- | --------- | ---------- |
| `default`    | `1.2.3`   | `^1.2.3`  | `^1.2.3` | `^1.2.3`  | `>=1.2.3` | `v1.2.3`   |
| `exact`      | `1.2.3`   | `1.2.3`   | `1.2.3`  | `1.2.3`   | `1.2.3`   | `v1.2.3`   |
| `caret`      | `1.2.3`   | `^1.2.3`  | `^1.2.3` | `^1.2.3`  | `>=1.2.3` | `v1.2.3`   |
| `compatible` | `>=1.2.3` | `>=1.2.3` | `^1.2.3` | `>=1.2.3` | `>=1.2.3` | `>=v1.2.3` |

Keep these limits in mind:

- `versions sync` never writes tilde (`~`) or equality (`=`) prefixes. To stamp internal dependency references with a custom prefix such as `~` or `=` at release time, use a typed `versioned_files` entry with an explicit `prefix` (see [Versioned files](../guide/04-configuration.md#versioned-files)).
- Deno keeps `^` under `compatible`, and Go's `compatible` output keeps the mandatory `v` module prefix (`>=v1.2.3`); prefer `default`, `exact`, or `caret` for those two ecosystems.
- Cargo reads a bare requirement such as `1.2.3` as a caret range, so `--strategy exact` does not produce a Cargo exact pin (`=1.2.3`).

## Supported ecosystems

`monochange versions` updates internal dependency constraints for all ecosystems monochange discovers: Cargo `Cargo.toml`, Dart `pubspec.yaml` / `pubspec.yml`, Deno `deno.json`, Go `go.mod`, npm `package.json`, and Python `pyproject.toml` manifests.

For Dart workspaces that use `resolution: workspace`, internal dependencies should use versioned constraints instead of `path:` references. `monochange versions` converts eligible internal `path:` references to the configured version constraint.
