---
monochange: feat
monochange_core: feat
"@monochange/skill": fix
---

# Honor ecosystems discovery settings and accept forgejo for init

`[ecosystems.<id>]` discovery settings now shape `monochange discover` and the release paths built on it:

- `enabled = false` skips that ecosystem entirely.
- `roots = ["crates", "packages"]` restricts discovery to packages whose manifests live under the listed directories.
- `exclude = ["tools/**"]` removes packages whose manifests match any glob. A glob that fails to compile warns once and matches nothing instead of silently dropping the filter.

Previously these fields were parsed but discovery scanned every supported ecosystem regardless. `monochange init --provider` also accepts `forgejo` alongside `github`, `gitlab`, and `gitea`.
