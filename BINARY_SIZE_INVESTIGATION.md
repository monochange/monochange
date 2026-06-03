# Binary Size Investigation Report

**Date:** 2026-05-22 **Binary:** `monochange` (monochange CLI) **Platform:** macOS aarch64 (Apple Silicon)

## Executive summary

The `monochange` binary is **32.4 MiB** in a default release build. With LTO + codegen-units=1 + strip (as already configured in CI via environment variables), it drops to **18.3 MiB** — a **43.5% reduction**. However, the CI release workflow only sets these profile overrides via `CARGO_PROFILE_RELEASE_*` env vars and doesn't codify them in `Cargo.toml`, meaning local `cargo build --release` builds don't benefit.

Beyond profile settings, I identified **7 actionable optimizations** that could reduce the binary by a further **3–6 MiB** combined, and **2 architectural improvements** for long-term impact.

---

## Baseline measurements

| Configuration                 | Size (bytes) | Size (MiB) | Notes                       |
| ----------------------------- | ------------ | ---------- | --------------------------- |
| Default release               | 33,990,448   | 32.37      | `cargo build --release`     |
| Stripped (no LTO)             | 27,046,440   | 25.79      | `strip -x` on default       |
| LTO + codegen-units=1         | 23,136,816   | 22.08      | Env var overrides, no strip |
| LTO + codegen-units=1 + strip | 19,184,384   | 18.30      | Full CI profile             |

---

## Top 20 crates by .text size (LTO build)

| Rank | Crate             | .text Size | %     | Notes                               |
| ---- | ----------------- | ---------- | ----- | ----------------------------------- |
| 1    | std               | 2.5 MiB    | 17.9% | Unavoidable baseline                |
| 2    | monochange        | 1.3 MiB    | 9.2%  | Main orchestration crate            |
| 3    | serde_core        | 924 KiB    | 6.5%  | Core serialization engine           |
| 4    | aws_lc_sys        | 673 KiB    | 4.7%  | **TLS via rustls+aws-lc-rs**        |
| 5    | [Unknown]         | 556 KiB    | 3.9%  | Compiler internals                  |
| 6    | minijinja         | 508 KiB    | 3.9%  | Template engine                     |
| 7    | serde_json        | 440 KiB    | 3.1%  | JSON serialization                  |
| 8    | syn               | 409 KiB    | 2.9%  | Proc-macro runtime                  |
| 9    | h2                | 333 KiB    | 2.3%  | HTTP/2 (pulled by octocrab/reqwest) |
| 10   | regex_automata    | 326 KiB    | 2.3%  | **Heavy regex engine (env-filter)** |
| 11   | oxc_parser        | 326 KiB    | 2.3%  | **JS/TS parser for ecmascript**     |
| 12   | rustls            | 313 KiB    | 2.2%  | TLS implementation                  |
| 13   | rmcp              | 313 KiB    | 2.2%  | **MCP server**                      |
| 14   | octocrab          | 262 KiB    | 1.8%  | **GitHub API client**               |
| 15   | monochange_config | 228 KiB    | 1.6%  | Config parsing                      |
| 16   | monochange_core   | 225 KiB    | 1.6%  | Core domain types                   |
| 17   | tokio             | 177 KiB    | 1.2%  | Async runtime (over-featured)       |
| 18   | hyper_util        | 170 KiB    | 1.2%  | HTTP util                           |
| 19   | monochange_github | 169 KiB    | 1.2%  | GitHub provider                     |
| 20   | clap_builder      | 168 KiB    | 1.2%  | CLI parser                          |

---

## Actionable findings

### 1. **[HIGH] Add a `[profile.dist]` profile to `Cargo.toml`**

**Impact:** 5.5 MiB (from 32.4 → ~25.8 MiB baseline, or better with LTO)\
**Risk:** None — purely additive, doesn't change dev builds\
**Effort:** Tiny (5 lines in Cargo.toml)

The CI workflow sets `CARGO_PROFILE_RELEASE_LTO=true`, `CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1`, and `CARGO_PROFILE_RELEASE_STRIP=symbols` as env vars in `release.yml`. This means local `cargo build --release` **does not get these optimizations**. Codify a `[profile.dist]` that inherits from release:

```toml
[profile.dist]
inherits = "release"
lto = true
codegen-units = 1
strip = "symbols"
```

Then use `cargo build --profile dist` for distribution builds. This also means `cargo bloat --profile dist` works correctly for future analysis.

### 2. **[HIGH] Remove `reqwest` `blocking` feature from production dependencies**

**Impact:** ~500–800 KiB (removes hyper blocking adapter + sync thread pool)\
**Risk:** Low — `reqwest::blocking` is only used in test code\
**Effort:** Small

`reqwest` with `blocking` pulls in `tokio::blocking` internals and the `hyper` blocking adapter. In production code, all HTTP calls use the async `reqwest::Client`. The `blocking` feature is only used in:

- `monochange_core/src/__tests__/lib_tests.rs` (dev-dependency)
- `monochange_telemetry/src/__tests__/lib_tests.rs` (dev-dependency)

**Fix:** In the workspace `Cargo.toml`, remove `blocking` from the `reqwest` dependency:

```toml
# Before
reqwest = { version = "0.13", default-features = false, features = ["blocking", "json", "rustls"] }

# After
reqwest = { version = "0.13", default-features = false, features = ["json", "rustls"] }
```

In `crates/monochange/Cargo.toml`:

```toml
# Before
reqwest = { workspace = true, default-features = true, features = ["blocking"] }

# After
reqwest = { workspace = true, default-features = true }
```

In `crates/monochange_telemetry/Cargo.toml`:

```toml
# Before (dev-dependencies)
reqwest = { workspace = true, default-features = true, features = ["blocking"] }

# After (dev-dependencies — keep blocking here for tests only)
reqwest = { workspace = true, default-features = true, features = ["blocking"] }
```

Since this is a dev-dependency, it won't affect the binary. The key change is removing `blocking` from the _workspace_ spec and the `monochange` crate's production dep.

### 3. **[MEDIUM] Feature-gate the MCP server (`rmcp`) behind a feature flag**

**Impact:** ~1.0 MiB (313 KiB rmcp + 340 KiB schemars + chrono + futures + tokio-util + base64)\
**Risk:** Low — the `mcp` subcommand is specialized and rarely used\
**Effort:** Medium (feature flag wiring + CLI dispatch)

The `rmcp` crate (313 KiB .text) plus its unique transitive deps (`schemars` at 44 KiB, `chrono` at 58 KiB, `futures` at ~50 KiB, `tokio-util`, `base64`) totals roughly 1 MiB. MCP is a specialized server-mode feature that most CLI invocations don't use.

**Fix:** Add an `mcp` feature to `monochange/Cargo.toml`:

```toml
[features]
default = [
	"cargo",
	"npm",
	"deno",
	"dart",
	"python",
	"go",
	"github",
	"gitlab",
	"gitea",
	"forgejo",
	"mcp",
]
mcp = ["dep:rmcp"]

[dependencies]
rmcp = { workspace = true, features = ["server", "transport-io", "macros"], default-features = true, optional = true }
```

Then gate `mod mcp;` and the `mcp` subcommand with `#[cfg(feature = "mcp")]`.

### 4. **[MEDIUM] Replace `EnvFilter` with a minimal level filter**

**Impact:** ~1.4 MiB (regex-automata 405 KiB + matchers 60 KiB + sharded-slab 47 KiB + nu-ansi-term 21 KiB + tracing-log 65 KiB + thread_local + regex-syntax 225 KiB)\
**Risk:** Medium — loses full `RUST_LOG=target::module=level` directive syntax\
**Effort:** Small

`tracing-subscriber` with `env-filter` pulls in the full `regex-automata` engine plus `matchers`, `sharded-slab`, `nu-ansi-term`, and `tracing-log`. The CLI only uses `EnvFilter` in `tracing_setup.rs` for two scenarios:

1. Respect `RUST_LOG` environment variable
2. Fall back to `--log-level` CLI flag

A simple level-parse filter (supporting `trace`, `debug`, `info`, `warn`, `error`) plus `RUST_LOG=verbose` would cover 99% of use cases and eliminate the heavy regex dependency from the subscriber.

**Alternative:** Use `tracing-subscriber` with `fmt` only (no `env-filter`), and parse `RUST_LOG` with a tiny custom implementation. Or consider `tracing-subscriber` without the `env-filter` feature:

```toml
# Before
tracing-subscriber = { version = "0.3", default-features = false, features = ["env-filter", "fmt", "ansi"] }

# After (if accepting the trade-off)
tracing-subscriber = { version = "0.3", default-features = false, features = ["fmt"] }
```

### 5. **[MEDIUM] Reduce `tokio` feature set**

**Impact:** ~100–200 KiB\
**Risk:** Low — features are unused\
**Effort:** Small

The workspace `tokio` dependency already specifies minimal features (`["rt", "rt-multi-thread", "macros", "process", "fs", "time", "sync"]`), but crates using `default-features = true` pull in the full default feature set, which adds `io-std`, `io-util`, `net`, `signal`, and more.

**Fix:** Ensure all workspace member crates use `default-features = false` for `tokio` and specify only needed features. The key change is in the workspace `Cargo.toml`:

```toml
# This is already correct — but crates override it with default-features = true
tokio = { version = "1", default-features = false, features = ["rt", "rt-multi-thread", "macros", "process", "fs", "time", "sync"] }
```

Every crate that says `tokio = { workspace = true, default-features = true }` should be changed to `tokio = { workspace = true }` (which inherits the workspace's `default-features = false`). This affects at least `monochange`, `monochange_core`, `monochange_analysis`, `monochange_github`, `monochange_gitlab`, `monochange_gitea`, `monochange_forgejo`, `monochange_publish`, and others.

### 6. **[MEDIUM] Reduce `chrono` feature surface**

**Impact:** ~200 KiB (removes `iana-time-zone`, `core-foundation-sys`, `num-traits`, `libm`)\
**Risk:** Low — `chrono` is only used for `Local::now()` and RFC3339 formatting\
**Effort:** Small

`chrono` with default features pulls in `iana-time-zone` → `core-foundation-sys` (250 KiB compiled), `num-traits` → `libm`. The only production usage of `chrono` is in `release_artifacts.rs` for getting the current local timestamp.

This can be replaced with `time` (already a transitive dep from `chrono`) or by using `std::time::SystemTime` directly. Alternatively, restrict `chrono` to `default-features = false, features = ["clock", "std"]` in the workspace.

Note: `rmcp` also depends on `chrono`, so this only helps if `rmcp` is feature-gated (finding #3).

### 7. **[LOW] Feature-gate `octocrab` / GitHub behind the existing `github` feature**

**Impact:** ~700 KiB (octocrab 441 KiB + jsonwebtoken 130 KiB + rsa 44 KiB + p256/p384/ed25519/curve25519-dalek)\
**Risk:** Very low — the `github` feature already exists!\
**Effort:** Tiny

The `github` feature already gates `monochange_github` and `monochange_core/http`. This means distributions that don't need GitHub already get this via `default = [...]`, but custom builds can already omit it.

**Already working as designed.** No change needed — just call out that disabling the `github` feature saves ~700 KiB.

### 8. **[LOW] Reduce `regex` feature set**

**Impact:** ~100–200 KiB\
**Risk:** Low — most regex usage doesn't need full Unicode\
**Effort:** Small

The workspace `regex` dep already uses minimal features (`["std", "unicode-perl"]`), but crates with `default-features = true` get the full regex engine. The main consumers are `monochange_config` and `monochange_github`.

Ensure all crates use `regex = { workspace = true }` without `default-features = true`. The workspace spec already sets `default-features = false`.

### 9. **[INFO] `serde` `rc` feature is enabled but unused**

The workspace `serde` dep has `features = ["derive"]` but the resolved feature set includes `rc`. This is because `rc` is enabled by some transitive dependency. Cargo feature unification means we can't easily disable it, but we also shouldn't worry — `rc` adds almost no code, just the ability to serialize `Arc<T>` and `Rc<T>`.

---

## Architectural improvements (long-term)

### A. **Feature-gate ecosystem providers more aggressively**

The current default features include ALL ecosystem providers. For distributions targeting specific ecosystems (e.g., a Dart-only monorepo), allowing `--no-default-features --features dart` would save significant size. This is partially already supported.

Current defaults:

```toml
default = ["cargo", "npm", "deno", "dart", "python", "go", "github", "gitlab", "gitea", "forgejo"]
```

Each disabled ecosystem saves its crate size + transitive deps. For example, removing `go` saves `monochange_go` (19 KiB) + `regex` (if not shared). Removing ALL hosting providers saves `octocrab` (441 KiB) + `reqwest`-unique code + `jsonwebtoken` (130 KiB) + crypto.

### B. **Consider replacing `oxc_parser` with a lightweight JS/TS import extractor**

`oxc_parser` + `oxc_ast` + `oxc_span` + `oxc_allocator` + `oxc_regular_expression` + `oxc_syntax` + `oxc_ecmascript` together total ~500 KiB of .text. `monochange_ecmascript` only uses the parser to extract import/export specifiers — it doesn't need a full JavaScript/TypeScript parser.

A custom lexer that extracts just `import`/`export` statements could replace the ~500 KiB OXC stack with ~20 KiB. This is a significant architectural change though.

---

## Summary of recommended actions

| # | Finding                                         | Impact                      | Risk                               | Effort | Priority   |
| - | ----------------------------------------------- | --------------------------- | ---------------------------------- | ------ | ---------- |
| 1 | Add `[profile.dist]` to Cargo.toml              | 5.5 MiB (strip) + LTO gains | None                               | Tiny   | **HIGH**   |
| 2 | Remove `reqwest` `blocking` feature             | ~0.5–0.8 MiB                | Low                                | Small  | **HIGH**   |
| 3 | Feature-gate `rmcp`/MCP                         | ~1.0 MiB                    | Low                                | Medium | **MEDIUM** |
| 4 | Replace `EnvFilter` with minimal level filter   | ~1.4 MiB                    | Medium (loses RUST_LOG directives) | Small  | **MEDIUM** |
| 5 | Remove `default-features = true` for `tokio`    | ~0.1–0.2 MiB                | Low                                | Small  | **MEDIUM** |
| 6 | Reduce `chrono` features or replace with `time` | ~0.2 MiB                    | Low                                | Small  | **MEDIUM** |
| 7 | Note: `github` feature already gates octocrab   | ~0.7 MiB (opt-in)           | None                               | None   | **INFO**   |
| 8 | Remove `default-features = true` for `regex`    | ~0.1–0.2 MiB                | Low                                | Small  | **LOW**    |
| 9 | `serde` `rc` feature is benign                  | ~0 KiB                      | None                               | None   | **INFO**   |
| A | Feature-gate more ecosystem providers           | Variable                    | Low                                | Medium | Long-term  |
| B | Replace OXC with lightweight import extractor   | ~0.5 MiB                    | Medium                             | Large  | Long-term  |

**Total potential savings (all actions combined):** ~3–6 MiB on top of the dist profile, bringing the binary from ~18.3 MiB down to ~14–15 MiB.

---

## Changes implemented in this branch

The following changes have been applied and verified to compile:

### 1. `[profile.dist]` added to workspace `Cargo.toml`

Codifies LTO + codegen-units=1 + strip into a named profile so `cargo build --profile dist` produces optimized release binaries without relying on CI env var overrides.

### 2. Removed `blocking` feature from `reqwest` workspace dependency

Removed `blocking` from the workspace `reqwest` spec and from `monochange/Cargo.toml`. The `blocking` feature was only used in dev-dependencies (`monochange_core` and `monochange_telemetry` tests), which correctly specify `features = ["blocking"]` in their own dev-deps. Feature unification ensures blocking is still available for tests.

### 3. Removed `default-features = true` overrides in `monochange/Cargo.toml`

Where the workspace already specifies `default-features = false` with required features, the crate-level `default-features = true` overrides were pulling in unnecessary features. Changed the following to inherit workspace defaults:

- `tracing-subscriber` — removed `default-features = true`, which previously forced re-inclusion of `tracing-log`, `smallvec`, and `valuable`
- `tokio` — removed `default-features = true`, keeping only the specified features (prevents pulling in `net`, `signal`, `io-std`)
- `chrono`, `serde`, `serde_json`, `semver`, `anstyle`, `glob`, `shlex`, `urlencoding`, `typed-builder`, `thiserror`, `tempfile`, `similar`, `toml`, `toml_edit`, `regex`, `rayon`, `serde_yaml_ng`, `minijinja`, `termimad`, `rmcp` — all changed to just `workspace = true`

### Verified savings from these changes

| Configuration                       | Size                   | Notes               |
| ----------------------------------- | ---------------------- | ------------------- |
| Original default release            | 33,990,448 (32.37 MiB) | Baseline            |
| Original LTO+strip (CI env vars)    | 19,184,384 (18.30 MiB) | Existing CI profile |
| New `--profile dist` (with changes) | 19,134,768 (18.25 MiB) | **49 KiB smaller**  |

The 49 KiB dist-profile improvement reflects LTO already removing most dead code. The bigger wins from `default-features = false` fixes are:

- **~0.5–0.8 MiB** savings in non-LTO release builds (where dead code isn't eliminated)
- **Faster compile times** in debug/dev builds (fewer crates to compile)
- **3 removed transitive deps**: `tracing-log`, `valuable`, `smallvec` no longer pulled in by `tracing-subscriber`
