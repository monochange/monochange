---
monochange_app: feat
---

# Add leptoskit Android and iOS app scaffolds

`app/native/` now holds a leptoskit workspace for the monochange Android and iOS clients. Native screens share `monochange_rpc` types with the future web port and call `leptoskit_rpc` for the auth handshake (`SessionProbe` / `auth_handshake`) and project list (`ListProjects`). UI chrome follows the Release floor `--mc-*` tokens (mist page, ink, teal accent) instead of the legacy violet marketing palette.

Build the hosts with the leptoskit CLI after installing platform SDKs:

```bash
cd app/native
leptoskit doctor
leptoskit native prepare android   # needs ANDROID_HOME, SDK 36, NDK
leptoskit build --platform android
leptoskit native build ios-simulator   # macOS + Xcode only
```

Generated `.leptoskit/` trees stay local. Release signing still needs `LEPTOSKIT_ANDROID_*` keystore secrets on Android and an Apple development team on iOS; this scaffold does not ship store credentials.
