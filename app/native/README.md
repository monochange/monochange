# monochange native (Android + iOS)

leptoskit guest apps that share RPC types with the web port and call `leptoskit_rpc` for the auth handshake and project list. Visuals follow the **Release floor** token set (`--mc-*`) from the landing design system.

## Layout

```text
app/native/
├── leptoskit.toml              # platforms: android, ios (+ web reserved)
├── Cargo.toml                  # workspace
├── monochange_rpc/             # shared serde + RPC ops (web port should path-dep this)
├── monochange_native_app/      # Element screens (guest wasm)
├── monochange_native_backend/  # RPC handlers + axum merge helpers
├── android/                    # Kotlin host (edit here; .leptoskit/ is generated)
└── ios/                        # UIKit host (edit here; .leptoskit/ is generated)
```

## Design tokens

Native Element trees use **Tailwind utilities** (slate + teal) that map to Release floor `--mc-*` values — see `monochange_native_app/src/theme.rs`. Prefer utilities such as `bg-teal-700` / `bg-slate-100` over inventing semantic CSS class stacks (`.mc-page`, `.button-brand`). Do not reintroduce the legacy violet marketing palette on mobile.

## Shared RPC

| Operation        | Purpose                                       |
| ---------------- | --------------------------------------------- |
| `auth_handshake` | Session probe: signed-in user or anonymous    |
| `list_projects`  | Flat project list for the signed-in workspace |

Types live in `monochange_rpc` so the web SSR port can import the same payloads without forking models.

## Toolchain requirements

| Target                   | Required on the build machine                                                                                                                                   |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Shared Rust / guest wasm | Rust nightly matching `rust-toolchain.toml`, `wasm32-unknown-unknown`, `leptoskit` CLI 0.3.x                                                                    |
| Android APK              | Java 17+, Android SDK **platform 36**, CMake, NDK, `ANDROID_HOME` (optional `ANDROID_NDK_HOME`), `rustup target add aarch64-linux-android x86_64-linux-android` |
| iOS app                  | **macOS** with Xcode + iOS Simulator runtime, `rustup target add aarch64-apple-ios-sim aarch64-apple-ios` (or `x86_64-apple-ios` on Intel)                      |

This Cloud Agent environment has **no** `ANDROID_HOME` and **no** Xcode. `cargo check -p monochange_rpc` (and host/lib crates without platform SDK steps) is what we can verify here. Full APK / `.app` output needs a machine with the SDK/secrets above.

Release signing secrets (not available here; set only on a signing host):

- Android: `LEPTOSKIT_ANDROID_KEYSTORE`, `LEPTOSKIT_ANDROID_KEYSTORE_PASSWORD`, `LEPTOSKIT_ANDROID_KEY_ALIAS`, `LEPTOSKIT_ANDROID_KEY_PASSWORD`
- iOS: Apple development team in Xcode **Signing & Capabilities**; App Store upload via `leptoskit publish app_store` with store credentials

## Develop / build (when toolchains are present)

```bash
cd app/native
# install CLI once (needs the repo nightly toolchain)
cargo install leptoskit_cli --version 0.3.0 --locked

leptoskit doctor                  # reports missing Android/iOS tools
leptoskit native prepare android  # regenerates .leptoskit/android/
leptoskit build --platform android
leptoskit native build ios-simulator   # macOS only
```

Point the guest at a running monochange server:

```bash
MONOCHANGE_SERVER=https://monochange.dev leptoskit build --platform android
```

Default origin is `http://127.0.0.1:3000` (use `adb reverse tcp:3000 tcp:3000` for an Android emulator).

## Status of this scaffold

- Host projects, shared RPC types, theme map, and guest screens are in-repo.
- Backend handlers return structured handshake / project-list payloads suitable for wiring to `monochange_app_db` once the web leptoskit port mounts the same RPC router.
- Generated `.leptoskit/` trees are **not** committed; run `leptoskit native
  prepare` before opening Android Studio or Xcode.

### Verified in this environment (2026-11-07)

```text
cargo test -p monochange_rpc --lib          # pass
cargo check -p monochange_rpc --features server
cargo check -p monochange_native_backend
cargo check -p monochange_native
cargo check -p monochange_native_app        # default rpc-client
cargo check -p monochange_android           # Linux host crate (no APK)
cargo check -p monochange_ios               # Linux host crate (no .app)
leptoskit doctor                            # fails: no ANDROID_HOME / Xcode
```

`leptoskit doctor` blockers on this Linux Cloud Agent image:

- Android: `ANDROID_HOME`, platform `android-36`, NDK, CMake, `aarch64-linux-android` Rust target, platform-tools/adb
- iOS: macOS + Xcode (hard requirement)
- Optional: `wasm-opt` (disabled in `leptoskit.toml` for this scaffold)
