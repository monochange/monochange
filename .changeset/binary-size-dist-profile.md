---
monochange: patch
monochange_core: patch
monochange_forgejo: patch
monochange_github: patch
monochange_gitea: patch
monochange_gitlab: patch
monochange_hosting: patch
monochange_publish: patch
monochange_telemetry: patch
monochange_test_helpers: patch
---

# Reduce binary size with dist profile, MCP feature gate, EnvFilter replacement, and ring TLS backend

- Add `[profile.dist]` for optimized CI/release builds (LTO, codegen-units=1, strip)
- Feature-gate `rmcp`/MCP server behind `mcp` feature (default-enabled, ~313 KiB savings when disabled)
- Replace `EnvFilter` with `LevelFilter` in tracing setup (~1.4 MiB savings from removing tracing-log and regex)
- Switch TLS backend from `aws-lc-rs` to `ring` (~2.5 MiB binary size reduction)
- Install ring crypto provider at startup (required for rustls-no-provider)
- Remove `default-features = true` on `reqwest` workspace references (was re-enabling default TLS)
- Wire `dist` profile into binary-size CI job and release workflow
- Remove redundant `CARGO_PROFILE_RELEASE_*` env vars from release workflow
- Add `build:dist` and `test:dist` devenv scripts for dist-profile validation
- Add `test_dist` CI job to run tests against dist-optimized build
- Add `build dist profile` step to CI build job (Linux only)
