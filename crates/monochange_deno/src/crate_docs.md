# `monochange_deno`

<!-- {=monochangeDenoCrateDocs} -->

`monochange_deno` discovers Deno packages and workspace members for the shared planner.

Reach for this crate when you need to scan `deno.json` or `deno.jsonc` files, expand Deno workspaces, and normalize Deno dependencies into `monochange_core` records.

## Why use it?

- discover Deno workspaces and standalone packages with one adapter
- normalize manifest and dependency data for cross-ecosystem release planning
- include Deno-specific import and dependency extraction in the shared graph

## Best for

- scanning Deno repos without adopting the full workspace CLI
- turning `deno.json` metadata into shared package and dependency records
- mixing Deno packages into a broader cross-ecosystem monorepo plan

## Public entry points

- `discover_deno_packages(root)` discovers Deno workspaces and standalone packages
- `DenoAdapter` exposes the shared adapter interface

## Scope

- `deno.json` and `deno.jsonc`
- workspace glob expansion
- normalized dependency and import extraction

<!-- {/monochangeDenoCrateDocs} -->
