---
monochange_app: fix
---

# Build the deployment image with Binaryen 132 so the website hydrates

The `Dockerfile` no longer installs Debian bookworm's `binaryen` package (version 108) for `wasm-opt`. Binaryen before `version_110` writes every table export with index 0, so the release modules exported `__wbindgen_externrefs` as the fixed-size function table, and the wasm-bindgen glue threw `RangeError: WebAssembly.Table.grow(): failed to grow table by 4` before any page hydrated. The image now installs the statically linked `wasm-opt` from the Binaryen `version_132` release, verified against its SHA-256 digest: the same Binaryen release devenv provides.

`app/deploy/verify-image.sh` now copies both shipped main modules (`pkg/site/monochange_app_bg.wasm` and `pkg/projects/monochange_projects_web_bg.wasm`) out of the running container and checks them with `scripts/check-wasm-externref-table.ts`, which fails unless `__wbindgen_externrefs` refers to an externref table that can grow by at least 4. The `app-ci` `docker` job and the `app` deployment job set up Node 24 to run it.
