---
monochange: fix
---

# Make hosted release commits match the files a local release commit stages

`CommitRelease` with `commit_backend = "hosted"` now honors `stage_all = true`. Before this change, the hosted backend ignored the setting and sent only the files monochange tracks itself (changelogs, manifests, consumed changesets, and the release record), so files that earlier `Command` steps rewrote, such as `Cargo.lock`, `pnpm-lock.yaml`, generated schemas, or CLI snapshots, were missing from the bot's release commit.

With `stage_all = true`, the hosted request now also carries every path `git add --all` would stage: modified and staged tracked files, untracked files that `.gitignore` does not exclude, and deleted files (sent without content so the release commit removes them). Workflows that leave `stage_all` unset still send only the release-managed files.

The hosted backend now also filters release-managed files the way the local backend does before `git add`. It no longer sends `.monochange/local/release-manifest.json` or other ignored local artifacts, which previously landed on the release branch, and it only sends a missing path as a deletion when git tracks it.

```toml
[[cli.release.steps]]
type = "CommitRelease"
commit_backend = "hosted"
inputs = { stage_all = true }
```
