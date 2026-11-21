---
monochange_app: refactor
---

# Cache the deployment image build between CI runs

The `Dockerfile` now resolves the leptoskit CLI pin in a separate `cli-pin` stage and copies only the resolved pin into the builder. Editing workspace crates no longer invalidates the CLI install layer; it rebuilds only when `[workspace.metadata.bin].leptoskit_cli` changes. The workspace sources are copied after the tool installs.

The `app-ci` `docker` job builds through Buildx with a GitHub Actions layer cache (`scope=app-image`) and restores the cargo registry and `app/target` cache mounts from the Actions cache, so a run recompiles only the crates that changed. The image contents, the smoke check, and the release-PR image export are unchanged.
