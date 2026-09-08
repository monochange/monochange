---
main: patch
---

# update dependencies and GitHub Actions

Direct crate dependencies were bumped to their latest versions: `octocrab` 0.54.1, `oxc_allocator`/`oxc_ast`/`oxc_parser`/`oxc_span` 0.149.0 (with the parser migration to the split `ExportDeclaration`/`ExportNamedDeclaration`/`ExportFromDeclaration` module declarations and `TSNamespaceDeclaration`/`TSExternalModuleDeclaration` replacing `TSModuleDeclaration`), `rmcp` 3.2 (tool results now use `ContentBlock` instead of the removed `Content` wrapper), `syn` 3.0.5, `jsonschema` 0.55, `termimad` 0.35, `shlex` 2, `rstest` 0.27, `insta-cmd` 0.7, and `time` 0.3.55. The `time` crate's exact-version pin (`=0.3.47`) was removed because the octocrab `From` impl coherence failure it guarded against no longer reproduces on octocrab 0.54; `cargo deny` now allows the `Zlib` license (`foldhash` via `jsonschema`'s `referencing`) and tracks the new duplicate `base64`/`num-bigint`/`syn` versions.

No public API changed; the crate manifest dependency requirements move with this patch release. GitHub Actions were also refreshed to their latest releases (checkout v7.0.1, upload-artifact v7.0.1, download-artifact v8.0.1, cache v6.1.0, setup-node v7.0.0, pages actions v5/v6, attest-build-provenance v4.2.2, zizmor-action v0.6.3, changed-files v47.0.6, rust-cache v2.9.2, install-nix-action v31.11.1, install-action v2.87.8, setup-cross-toolchain v1.42.0, crates-io-auth-action v1.0.5, pnpm/action-setup v6.1.0, and the dtolnay/rust-toolchain branch pins), all still pinned by commit hash.
