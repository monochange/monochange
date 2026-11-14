---
monochange_app: fix
---

# Fix CI web build tool preparation and scratch paths

The benchmark job now installs Tailwind before recording build-tool provenance, resolves the workspace-pinned esbuild through `pnpm exec`, and uses one repository-rooted `.monochange/local/web-build` scratch directory for both frontend builds and publication.
