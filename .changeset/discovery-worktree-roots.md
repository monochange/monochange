---
monochange_core: fix
---

# Discover packages in workspaces nested under ignored directory names

`DiscoveryPathFilter` checked every component of the absolute manifest path against the skipped-directory list, so a workspace checked out below a directory named `.claude`, `node_modules`, `target`, `.git`, `.devenv`, `.fvm`, `.repos`, or `book` discovered zero packages. Claude Code worktrees (`.claude/worktrees/<name>`) were the common case: `monochange discover` reported an empty workspace and `monochange check` linted nothing.

Only components below the workspace root are checked now, matching how gitignore matching already worked, so the same repository discovers the same packages wherever it is checked out. Ignored directory names inside the workspace still filter discovery.
