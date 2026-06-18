---
monochange: patch
monochange_cargo: patch
monochange_core: patch
monochange_dart: patch
monochange_deno: patch
monochange_forgejo: patch
monochange_gitea: patch
monochange_github: patch
monochange_gitlab: patch
monochange_npm: patch
monochange_publish: patch
monochange_test_helpers: patch
---

# Move rate-limit policy planning into publish core

Keep `monochange` as the CLI crate while moving publish rate-limit policy and window planning helpers into `monochange_publish`.

Ecosystem manifest update planning now lives in the relevant ecosystem crates with `monochange` acting as the CLI orchestrator. Hosted-source adapters now own release URL and release request planning behavior. The test helper crate also centralizes binary lookup for integration tests that need the `monochange` executable. `monochange_github` constrains the GitHub client transitive dependency set so release-job lockfile regeneration stays compatible with the pinned nightly toolchain.
