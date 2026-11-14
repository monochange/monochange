---
monochange_app: fix
---

# Compile navbar account-menu DOM work only for wasm

The account menu's `close` method previously hid its browser-only DOM operations behind the repository's custom patch-coverage ignore markers. Host tests resolve the disclosure `NodeRef`s to `None` and return early, so those lines could never execute under native coverage, while Codecov did not honor the custom markers. The DOM work now lives behind the same `cfg(target_arch = "wasm32")` boundary already used by the route-change effect: native SSR and host tests keep the noop contract, browser behavior is unchanged, and the custom ignore markers are removed. The web navigation verification journey now also exercises the real desktop account disclosure (open, focus inside, Escape closes and restores focus), an Escape on an already-closed menu that does not steal focus, mobile Escape dismissal, and route-change menu/panel collapse.
