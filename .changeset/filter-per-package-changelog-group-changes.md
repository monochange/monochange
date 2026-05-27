---
monochange_changelog: fix
---

# Filter group-propagated changes from per-package changelogs

When a package is a member of a version group, its per-package changelog now only includes changes from changesets that directly target that package (kind=Package), not changes propagated from group-level targeting (kind=Group). Group-level changes appear exclusively in the group changelog, eliminating content duplication across member changelogs.
