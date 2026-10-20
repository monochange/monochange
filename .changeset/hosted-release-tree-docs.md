---
monochange_app: website_fix
---

# Simpler hosted release setup

The GitHub App guide now shows a release job that switches to the hosted backend with one environment variable, `MONOCHANGE_RELEASE_BACKEND=hosted`, instead of editing `monochange.toml`. It also explains why the job needs `contents: write`: the CLI uploads the release files with the job's own token, and monochange.dev only receives a reference to them.
