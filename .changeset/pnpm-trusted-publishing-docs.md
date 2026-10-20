---
monochange_app: fix
---

# Document pnpm trusted publishing

The trusted-publishing and configuration guides now explain that pnpm workspaces publish with `pnpm publish`, including under trusted publishing, that this requires pnpm 11.1.3 or later on the publishing runner, and why monochange passes `--no-git-checks` to pnpm.
