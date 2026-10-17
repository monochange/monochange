---
monochange_config: breaking
---

# ChangesetLoadContext borrows configured change types

`ChangesetLoadContext.change_types_by_target` now stores `ChangeTypeLookup<'a>`, which borrows change-type names from the workspace configuration instead of cloning `String` vectors for every context build. The field is public API, so code that names this type explicitly must update the lifetime. The same change removes four `realpath` syscalls per package from every changeset load, cutting changeset loading time by 84–96% on the benchmark workspaces.
