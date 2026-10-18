---
monochange: fix
---

# Print workspace-relative paths in `versions` output through symlinked roots

`monochange versions` and `monochange versions sync` printed absolute manifest paths in their text output whenever the workspace root was reached through a symlink, because the renderer trimmed the root exactly as it was given instead of comparing normalized paths. On macOS every workspace checked out under a `/var/folders` temporary directory hit this, so file headings showed full paths such as `/private/var/folders/.../packages/version_mismatch/pubspec.yaml` instead of `packages/version_mismatch/pubspec.yaml`.

Path headings and skipped-ecosystem entries now relativize against the normalized root, so the text output stays workspace-relative no matter how the root path is spelled. The JSON output is unchanged.
