---
monochange_app: patch
monochange_extension: patch
---

# Patch url stack overflow advisory

Bump `url` from 2.5.8 to 2.5.9 in `Cargo.lock`, `app/Cargo.lock`, and `extension/Cargo.lock`. Cargo also moves `form_urlencoded` from 1.2.2 to 1.2.3, the version `url` 2.5.9 requires.

2.5.8 parses the path of a `blob:` URL as another URL and calls `Url::origin()` on it without a nesting limit, so 10,000 nested `blob:` prefixes overflow a 2 MiB thread stack and abort the process (RUSTSEC-2026-0592). 2.5.9 only takes the inner origin for `http`, `https`, and `file` schemes. Patch-level move inside the existing `url = "2"` requirement; no source changes.
