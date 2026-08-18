---
monochange: patch
---

# Skip the second workspace configuration load during `step prepare-release`

`step prepare-release` (and the `DisplayVersions` step) previously loaded the full workspace configuration twice: once for CLI command dispatch and again inside release planning. In repositories with many packages or broad `versioned_files` globs, each load re-expands workspace globs and re-validates package definitions, so the redundant load doubled the startup cost.

Release planning now accepts the already-loaded configuration, so the CLI passes it through instead of reloading. The public `prepare_release` API keeps loading the configuration itself, and phase timing output is unchanged.
