---
monochange_npm: fix
---

# Infer root lockfiles for configured npm-family packages

`load_configured_npm_package` anchored configured packages to their manifest directory, so release preparation never inferred the workspace-level `pnpm-lock.yaml`, `package-lock.json`, or `yarn.lock`; those lockfiles only updated through explicit `versioned_files` entries or configured `lockfile_commands`. Package-manager detection had the same blind spot for `bun.lock` and `yarn.lock`.

The loader now resolves the workspace root by walking up to the nearest ancestor declaring `pnpm-workspace.yaml` or a `package.json` with `workspaces`, mirroring the cargo adapter, and recognizes `bun.lock` and `yarn.lock` when detecting the manager. Root lockfiles are rewritten directly during releases, and default lockfile commands run from the workspace root.
