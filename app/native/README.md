# monochange projects native clients

The iOS and Android guests use the same `monochange_shared` DTOs and `ApiClient` as the extension. The website owns GitHub sign-in and repository connection. Native clients receive a revocable, read-only device credential for that account through browser consent; they do not run a separate backend.

The home screen restores its credential from secure-store, loads real organizations and projects, and opens project details. A rejected credential clears local state. Network failures remain visible without silently signing out. Device sign-out attempts remote revocation and clears only that device's credential.

The guest uses upstream web-auth, crypto, HTTP, secure-store, and linking plugins. Browser state and the PKCE verifier stay in memory; the validated callback is consumed once before exchange. Secure-store failure is shown to the user. Adding these plugins requires rebuilding and redistributing the native hosts. This is not an OTA-only update.

`MONOCHANGE_SERVER` selects the shared website API at guest build time and defaults to `https://monochange.dev`. Add another exact origin to `[native.plugin-options.http].origins` when using a local fixture. The default callback is `dev.monochange.app:/auth/native/return`; backend overrides belong in `MONOCHANGE_NATIVE_REDIRECT_URIS`.

Run builds through the devenv shell and mbx Cargo shim. Use the repository-managed Leptoskit CLI at the revision pinned in root Cargo metadata. The platform commands are `leptoskit native build ios-simulator` and `leptoskit native build android` for arm64 Android emulators/devices (`android-x86_64` for an x86 emulator). Xcode, the Android SDK/NDK, and the matching Rust targets must be available. Inspect `leptoskit doctor --all` in that environment before changing toolchains.

The root package only anchors host packaging. `ios/` and `android/` register the configured plugins; generated hosts and local artifacts remain under ignored `.leptoskit/` and managed build directories.
