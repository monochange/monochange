---
monochange: patch
---

Reduce binary size with dist profile, MCP feature gate, and EnvFilter replacement

- Add `[profile.dist]` for optimized CI/release builds (LTO, codegen-units=1, strip)
- Feature-gate `rmcp`/MCP server behind `mcp` feature (enabled by default, ~1 MiB savings when disabled)
- Replace `EnvFilter` with `LevelFilter` in tracing setup (~1.4 MiB savings from removing `tracing-log` and regex parser)
- Wire `dist` profile into binary-size CI job and `taiki-e/upload-rust-binary-action` release workflow
- Remove redundant `CARGO_PROFILE_RELEASE_*` env vars from release workflow (now codified in `[profile.dist]`)