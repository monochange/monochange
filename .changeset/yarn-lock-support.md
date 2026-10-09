---
monochange_npm: breaking
---

# Add yarn.lock support for Yarn Classic and Berry workspaces

The npm-family adapter now recognizes `yarn.lock` as a versioned lockfile: release preparation discovers it, rewrites resolved version pins directly, and `yarn install --mode=update-lockfile` is the default `lockfile_commands` override when you prefer package-manager resolution.

Adding the `YarnLock` variant to the public `NpmVersionedFileKind` enum is the breaking surface: downstream exhaustive matches must gain the new arm (mark the enum `#[non_exhaustive]` in a follow-up if variant additions should stop requiring major releases).

`monochange_npm::update_yarn_lock` walks entry headers in both layouts: Classic (`version "1.2.3"` under `"name@range":` keys) and Berry (`version: 1.2.3` under descriptor keys such as `"name@npm:1.2.3"`). Workspace-resolved entries keep their `0.0.0-use.local` placeholder, and comments, checksums, resolved URLs, key ordering, and quoting are preserved because only the version value span is rewritten. Configure typed entries as `{ path = "yarn.lock", type = "npm" }`.

```rust
match monochange_npm::supported_versioned_file_kind(path) {
    Some(monochange_npm::NpmVersionedFileKind::YarnLock) => {
        // new: yarn.lock is rewritten directly during releases
    }
    // every exhaustive match on NpmVersionedFileKind needs this arm now
    _ => {}
}
```
