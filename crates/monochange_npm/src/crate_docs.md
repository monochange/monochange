# `monochange_npm`

<!-- {=monochangeNpmCrateDocs} -->

`monochange_npm` discovers npm-family packages and normalizes them for shared planning.

Reach for this crate when you want one adapter for npm, pnpm, and Bun workspaces that emits `monochange_core` package and dependency records.

## Why use it?

- discover several JavaScript package-manager layouts with one crate
- normalize workspace metadata into the same graph used by the rest of `monochange`
- capture dependency edges from `package.json` and `pnpm-workspace.yaml`

## Best for

- scanning JavaScript or TypeScript monorepos into normalized package records
- supporting npm, pnpm, and Bun with one discovery surface
- feeding JS workspace topology into shared planning code

## Public entry points

- `discover_npm_packages(root)` discovers npm, pnpm, and Bun workspaces plus standalone packages
- `NpmAdapter` exposes the shared adapter interface

## Scope

- `package.json` workspaces
- `pnpm-workspace.yaml`
- Bun lockfile detection
- normalized dependency extraction

<!-- {/monochangeNpmCrateDocs} -->
