---
monochange: patch
---

# Promote common workflow steps to top-level CLI commands

Add top-level aliases for commonly used built-in workflow steps, including `create`, `discover`, `config`, `preview`, `prepare`, `affected`, and `diagnose`. The new `preview` command runs the prepare-release workflow in dry-run mode so users can inspect planned release artifacts without writing files.
