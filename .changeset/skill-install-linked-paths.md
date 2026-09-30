---
monochange: patch
---

# Protect existing files during skill installation

Skill installation checks the selected destination and every bundled document path for existing symbolic links before writing. Linked documents, linked subdirectories, and dangling links are rejected even with `--force`, preventing an installation from overwriting files outside the selected tree. Linked parent directories above an explicitly selected destination remain supported.

```sh
monochange skill install --dir .agents/skills/monochange --force
```

If a path inside this destination is linked, the command now reports that path and leaves the skill tree untouched. Choose a regular destination or deliberately remove the link before retrying.
