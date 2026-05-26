# Internal dependency versions

Use `mc versions` to keep internal workspace dependency constraints aligned with each package's canonical version. It is useful when migrating an existing monorepo to monochange and when package versions change during normal release work.

```sh
mc versions --dry-run
mc versions
```

The command scans discovered workspace packages, builds a package-name to version map, and updates supported manifest files where one workspace package depends on another. It only syncs internal workspace dependencies; it does not change external dependency constraints.

## Output

By default, `mc versions --dry-run` prints the file and dependency updates it would make:

```text
would update ^1.1.0 → ^1.2.3 in core (packages/app/pubspec.yaml)

Strategy: default (package config → ecosystem config → ecosystem default; --strategy overrides)

(dry run — no files were modified)
```

Use JSON output for scripts and CI checks:

```sh
mc versions --dry-run --format json
```

The JSON result includes whether changes were applied, the selected strategy, changed files, dependency updates, and unsupported packages skipped during planning.

## Strategy precedence

`--strategy` controls the constraint style written for updated dependencies:

- `default` uses the configured package strategy first, then the ecosystem strategy, then the ecosystem default.
- `exact` writes the package version exactly, such as `1.2.3`.
- `caret` writes a caret constraint, such as `^1.2.3`.
- `compatible` writes the ecosystem-compatible constraint style.

The default fallback order is:

1. per-package setting
2. ecosystem config
3. ecosystem default

Passing `--strategy` overrides that fallback for the whole command.

## Supported ecosystems

`mc versions` currently updates Dart `pubspec.yaml` / `pubspec.yml` files and npm `package.json` files. Unsupported ecosystems are reported as skipped so migration runs show what still needs manual review.

For Dart workspaces that use `resolution: workspace`, internal dependencies should use versioned constraints instead of `path:` references. `mc versions` converts eligible internal `path:` references to the configured version constraint.
