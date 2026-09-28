---
"monochange": fix
---

# Point generated subagent guidance at `monochange skill`

`monochange subagents` generated agent files that never mentioned the bundled skill and taught only the `monochange step <name>` forms. The shared instructions written to the Claude, VS Code, GitHub Copilot, Pi, Codex, and Cursor targets now open by telling the agent to read the skill with `monochange skill` and `monochange skill read monochange`, and name the focused topics (`commands`, `changesets`, `change-classification`, `configuration`, `multi-package-publishing`) plus `monochange skill install --dir <dir>`.

The inspection list and recommended workflow now use the first-class commands (`monochange discover`, `monochange diagnose`, `monochange preview`, `monochange next`, `monochange versions list|sync`, `monochange publish packages|readiness|placeholder`) while keeping `monochange step <name>` as the portable fallback that `[cli.*]` step types bind to. Command choice follows the preferred order: the configured `monochange run <name>` workflow when `monochange run --help` lists it, then the short built-in, then `monochange step <name>`.

Two stale claims are corrected at the same time: the classification report field is `existing_changesets` (not `existingChangesets`), and configured workflows are listed by `monochange run --help` (or `monochange help run`), not by bare `monochange help`.

Regenerate existing files with `monochange subagents --all --force` to pick up the new text.
