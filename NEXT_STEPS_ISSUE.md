---
title: Reduce monochange CLI binary size
labels: ["improvement", "area:ci"]
---

## Problem

The `mc` binary is currently ~18.3 MiB in the optimized dist profile (LTO + codegen-units=1 + strip). While the initial `dist` profile and dependency cleanup (#521) brought it down from 32.4 MiB, there are several remaining opportunities to reduce the size further by 3–6 MiB without sacrificing functionality.

## Current bloat breakdown (top 10, dist profile)

| Crate                | .text Size | %     | Actionable?                       |
| -------------------- | ---------- | ----- | --------------------------------- |
| `std`                | 2.5 MiB    | 17.9% | No                                |
| `monochange`         | 1.3 MiB    | 9.2%  | No (main crate)                   |
| `serde_core`         | 924 KiB    | 6.5%  | No                                |
| `aws_lc_sys`         | 673 KiB    | 4.7%  | Only with TLS backend swap        |
| `minijinja`          | 508 KiB    | 3.9%  | Template engine — hard to replace |
| `serde_json`         | 440 KiB    | 3.1%  | No                                |
| `syn`                | 409 KiB    | 2.9%  | Proc-macro runtime                |
| `h2`                 | 333 KiB    | 2.3%  | With octocrab removal             |
| **`regex_automata`** | 326 KiB    | 2.3%  | **Yes — see env-filter below**    |
| **`oxc_parser`**     | 326 KiB    | 2.3%  | **Yes — lightweight extractor**   |

## Proposed next steps

### 1. Feature-gate the MCP server (`rmcp`) — ~1.0 MiB savings

The `rmcp` crate (313 KiB) plus its unique transitive deps (`schemars`, `chrono`, `futures`, `tokio-util`, `base64`) totals ~1 MiB. The `mc mcp` subcommand is a specialized server-mode feature that most CLI invocations don't use. Add an `mcp` feature to `monochange/Cargo.toml`, gate `mod mcp` and the `mcp` subcommand with `#[cfg(feature = "mcp")]`, and include `mcp` in the default features so existing behavior is preserved.

### 2. Replace `EnvFilter` with a minimal level filter — ~1.4 MiB savings

`tracing-subscriber` with `env-filter` pulls in `regex-automata` (405 KiB), `matchers` (60 KiB), `sharded-slab` (47 KiB), `nu-ansi-term` (21 KiB), and `tracing-log` (65 KiB). The CLI only uses `EnvFilter` in `tracing_setup.rs` for `RUST_LOG` and `--log-level`. A simple level-parse filter supporting `trace/debug/info/warn/error` plus `RUST_LOG=verbose` and `RUST_LOG=quiet` would cover 99% of use cases. Alternative: use `tracing-subscriber` without the `env-filter` feature.

### 3. Reduce `chrono` feature surface or replace with `time` — ~200 KiB savings

`chrono` with default features pulls in `iana-time-zone` → `core-foundation-sys` (250 KiB compiled), `num-traits` → `libm`. The only production usage is `chrono::Local::now().naive_local()` in `release_artifacts.rs`. Replace with `std::time::SystemTime` or the `time` crate (already a transitive dep). Note: `rmcp` also depends on `chrono`, so this only helps if `rmcp` is feature-gated first.

### 4. Replace `oxc_parser` with a lightweight JS/TS import extractor — ~500 KiB savings

`oxc_parser` + `oxc_ast` + `oxc_span` + `oxc_allocator` + `oxc_regular_expression` + `oxc_syntax` + `oxc_ecmascript` together total ~500 KiB. `monochange_ecmascript` only uses the parser to extract import/export specifiers — it doesn't need a full JavaScript/TypeScript parser. A custom lexer that extracts just `import`/`export` statements could replace the entire OXC stack with ~20 KiB.

### 5. Audit and reduce `default-features = true` across workspace crates

The initial cleanup in #521 covered the `monochange` crate. Other workspace crates (e.g., `monochange_core`, `monochange_publish`, `monochange_hosting`) still use `default-features = true` for `tokio`, `reqwest`, and other deps, which may contribute to feature unification pulling in unnecessary code.

## Acceptance criteria

- [ ] MCP server is feature-gated and excluded from minimal builds
- [ ] `EnvFilter` replaced or feature-gated; binary shrinks by ≥1 MiB
- [ ] `chrono` usage replaced or features minimized
- [ ] OXC parser replacement evaluated (may defer to separate issue)
- [ ] `default-features = true` audited across all workspace crates
- [ ] Binary size stays under the baseline set by the dist profile improvements
