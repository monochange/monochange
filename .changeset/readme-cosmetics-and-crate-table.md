---
"@monochange/cli": patch
"@monochange/skill": patch
monochange: patch
---

# polish the monochange readme layout and shorten headings

The monochange readme now centers its title, logo, badges, and intro blockquote at the top of the page, uses `<br />` spacing before headings and after every section for more breathing room between sections, and shortens every section heading to one or two Title Case words.

The workspace crate catalog is now a table with one row per crate: the crate name, its crates.io and docs.rs badge links, and a short description, replacing the nested bullet list.

```markdown
# before

## Command and automation matrix

- `monochange`: end-user CLI and orchestration layer for discovery, planning, and CLI-defined release commands.
  - [![Crates.io](…)](…) [![Docs.rs](…)](…)

# after

## Commands

| Crate        | Badges                                  | Description                                                                                     |
| ------------ | --------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `monochange` | [![Crates.io](…)](…) [![Docs.rs](…)](…) | end-user CLI and orchestration layer for discovery, planning, and CLI-defined release commands. |
```

The `Repository development` section is renamed to `Contributing`, and shared documentation blocks were renamed with it (`Quick CLI workflow` becomes `Quick Start`, which also shortens the `monochange --help` long help heading). The regenerated npm `@monochange/cli` README and `@monochange/skill` docs inherit the same table and heading updates through the shared `mdt` blocks.
